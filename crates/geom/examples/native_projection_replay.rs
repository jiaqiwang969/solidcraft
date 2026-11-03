//! Read-only projection of an already-recorded native drawing snapshot, for diagnostics.
use std::error::Error;
use std::fs::OpenOptions;
use std::io::Write;

use serde::Deserialize;
use serde_json::json;
use solidcraft_geom::{Vec3, brep::Brep, hlr, section::CurveAccuracy};

#[derive(Deserialize)]
struct EdgeAccuracy {
    accuracy: CurveAccuracy,
}

#[derive(Deserialize)]
struct Snapshot {
    brep: Brep,
    edges: Vec<EdgeAccuracy>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let source = args.get(1).ok_or("usage: native_projection_replay snapshot.json new-output-directory trim-samples")?;
    let out = args.get(2).ok_or("missing output directory")?;
    let trim_samples: usize = args.get(3).ok_or("missing trim-samples")?.parse()?;
    let snapshot: Snapshot = serde_json::from_reader(std::fs::File::open(source)?)?;
    let accuracy: Vec<_> = snapshot.edges.iter().map(|e| e.accuracy).collect();
    let opts = hlr::Options { trim_samples, max_tracks: 10_000, max_rays: 2_000_000, ..Default::default() };
    let p = hlr::prepare_with_accuracy(&snapshot.brep, Some(&accuracy), &opts, &|| false)?;
    std::fs::create_dir(out)?;
    let write = |name: &str, value: &serde_json::Value| -> Result<(), Box<dyn Error>> {
        let mut f = OpenOptions::new().write(true).create_new(true).open(std::path::Path::new(out).join(name))?;
        serde_json::to_writer(&mut f, value)?;
        f.write_all(b"\n")?;
        Ok(())
    };
    let report = serde_json::to_value(&p.report)?;
    write("prepare-report.json", &report)?;
    let defs = [p];
    let scene = [hlr::Placement { body: 0, rotation: [Vec3::X, Vec3::Y, Vec3::Z], translation: Vec3::ZERO }];
    let mut views = Vec::new();
    for (id, direction) in [("main", -Vec3::Z), ("a_end", Vec3::X), ("g_end", -Vec3::X)] {
        let view = hlr::View::new(direction, Vec3::Y)?;
        let start = std::time::Instant::now();
        let result = hlr::hidden_lines(&defs, &scene, &view, &opts, &|| start.elapsed().as_secs() > 180);
        let data = match result {
            Ok(d) => serde_json::to_value(d)?,
            Err(e) => json!({"error":e.to_string()}),
        };
        write(&format!("{id}.json"), &data)?;
        views.push(json!({"id":id,"x":view.x,"y":view.y,"direction":view.d,"elapsed_seconds":start.elapsed().as_secs_f64(),"stats":data.get("stats"),"error":data.get("error")}));
    }
    let result = json!({"source_snapshot":source,"trim_samples":trim_samples,"prepare_report":report,"views":views});
    write("result.json", &result)?;
    println!("{result}");
    Ok(())
}
