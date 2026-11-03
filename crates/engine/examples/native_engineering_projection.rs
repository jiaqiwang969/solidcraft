//! Bounded native design -> exact-source drawing capture for the typed engineering bridge.
//! No document writes, reconstruction from dimensions, or drawing adoption.
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::Write;

use serde_json::json;
use solidcraft_engine::{Session, io};
use solidcraft_geom::{Vec3, hlr};

fn require_complete_timeline(session: &Session) -> Result<(), Box<dyn Error>> {
    for feature in &session.model.results {
        if let Some(problem) = feature.error.as_ref().or(feature.warning.as_ref()) {
            return Err(format!("incomplete drawing source: {}: {problem}", feature.name).into());
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 2 && args.get(1).is_some_and(|s| s == "--version") {
        println!("solidcraft-engineering-projection/1");
        return Ok(());
    }
    if args.len() != 3 {
        return Err("usage: native_engineering_projection design.sc new-output-directory".into());
    }
    let path = args.get(1).ok_or("missing native design")?;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > 16 * 1024 * 1024 {
        return Err("native design must be a regular file within 16 MiB".into());
    }
    let out = args.get(2).ok_or("missing new output directory")?;
    std::fs::create_dir(out)?;
    let write = |name: &str, value: &serde_json::Value| -> Result<(), Box<dyn Error>> {
        let mut f = OpenOptions::new().write(true).create_new(true).open(std::path::Path::new(out).join(name))?;
        let bytes = serde_json::to_vec(value)?;
        if bytes.len() >= 16 * 1024 * 1024 {
            return Err("native output exceeds 16 MiB".into());
        }
        f.write_all(&bytes)?;
        f.write_all(b"\n")?;
        Ok(())
    };
    let (doc, saved) = io::read_design_file(path, None::<fn(io::DesignPreview)>)?;
    let mut s = Session::opened_at(doc, saved.as_ref(), Some(path));
    let inspection = s.execute("document.inspect", &json!({"measure":false}))?;
    write("document.json", &inspection)?;
    require_complete_timeline(&s)?;
    let state = s.world_state();
    if state.bodies.len() != 1 {
        return Err("diagnostic requires exactly one body".into());
    }
    let body = state.bodies.first().ok_or("missing body")?;
    write("native-names.json", &json!({"faces":solidcraft_doc::naming::face_names(body),"edges":solidcraft_doc::naming::edge_names(body)}))?;
    let brep = solidcraft_kernel::portable_body(&body.body)?;
    let raw = serde_json::to_value(&brep)?;
    write("portable.json", &raw)?;
    let mut curves = BTreeMap::<String, usize>::new();
    let mut surfaces = BTreeMap::<String, usize>::new();
    for sh in raw.get("shells").and_then(|x| x.as_array()).ok_or("shells")? {
        for (field, member, counts) in [("edges", "curve", &mut curves), ("faces", "surface", &mut surfaces)] {
            for v in sh.get(field).and_then(|x| x.as_array()).ok_or("records")? {
                let kind = v.get(member).and_then(|v| v.get("kind")).and_then(|v| v.as_str()).ok_or("kind")?;
                *counts.entry(kind.to_owned()).or_default() += 1;
            }
        }
    }
    let opts = hlr::Options { trim_samples: 1024, max_tracks: 10_000, max_rays: 2_000_000, ..Default::default() };
    let adapted = Some(solidcraft_kernel::drawing_body(&body.body, 1e-6)?);
    let accuracy: Vec<_> = adapted.as_ref().map(|a| a.edges.iter().map(|e| e.accuracy).collect()).unwrap_or_default();
    if let Some(a) = &adapted {
        write("adapted-source.json", &serde_json::to_value(a)?)?;
        eprintln!("{}", json!({"stage":"adapted","evaluations":a.evaluations,"edges":a.edges.len(),"faces":a.faces.len()}));
    }
    let prepared = match &adapted {
        Some(a) => hlr::prepare_with_accuracy(&a.brep, Some(&accuracy), &opts, &|| false),
        None => hlr::prepare(&brep, &opts),
    };
    let result = match prepared {
        Err(e) => return Err(e.into()),
        Ok(p) => {
            let report = serde_json::to_value(&p.report)?;
            write("prepare-report.json", &report)?;
            eprintln!("{}", json!({"stage":"prepared","report":report}));
            let scene = [hlr::Placement { body: 0, rotation: [Vec3::X, Vec3::Y, Vec3::Z], translation: Vec3::ZERO }];
            let defs = [p];
            let mut views = Vec::new();
            for (id, direction) in [("main", -Vec3::Z), ("a_end", Vec3::X), ("g_end", -Vec3::X)] {
                let v = hlr::View::new(direction, Vec3::Y)?;
                let start = std::time::Instant::now();
                let r = hlr::hidden_lines(&defs, &scene, &v, &opts, &|| start.elapsed().as_secs() > 180);
                let data = match r {
                    Ok(d) => serde_json::to_value(d)?,
                    Err(e) => return Err(e.into()),
                };
                write(&format!("{id}.json"), &data)?;
                views.push(json!({"id":id,"x":v.x,"y":v.y,"direction":v.d,"elapsed_seconds":start.elapsed().as_secs_f64(),"stats":data.get("stats"),"error":data.get("error")}));
            }
            json!({"prepare_report":report,"views":views})
        }
    };
    write(
        "result.json",
        &json!({"schema":"solidcraft.engineering-projection/1","profile":"solidcraft-engineering-projection/1","source":path,"body":body.name,"recomputed":s.model.last_recomputed,"options":{"trim_samples":1024,"max_tracks":10000,"max_rays":2000000,"sample_deviation_mm":1e-6,"tangent_edges":true,"freeform":false},"curves":curves,"surfaces":surfaces,"result":result}),
    )?;
    println!("{}", json!({"curves":curves,"surfaces":surfaces,"result":result}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_source_rejects_a_failed_feature_even_when_a_body_remains() {
        let mut session = Session::default();
        session.execute("solid.box", &json!({"length":20,"width":10,"height":5})).unwrap();
        require_complete_timeline(&session).unwrap();
        // A loaded document can retain the body from an earlier feature when a
        // later reference no longer resolves. Capturing that body silently loses
        // the intended feature. The diagnostic must refuse errors and warnings.
        assert_eq!(session.model.state().bodies.len(), 1);
        session.model.results.last_mut().unwrap().error = Some("reference no longer resolves".into());
        assert!(require_complete_timeline(&session).unwrap_err().to_string().contains("reference no longer resolves"));
        let result = session.model.results.last_mut().unwrap();
        result.error = None;
        result.warning = Some("cutter was displaced".into());
        assert!(require_complete_timeline(&session).unwrap_err().to_string().contains("cutter was displaced"));
    }
}
