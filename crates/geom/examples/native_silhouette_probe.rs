//! Bounded read-only observation of the same source tracks used by native projection.
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;
use solidcraft_geom::{Vec3, brep::Brep, hlr, section::CurveAccuracy};
use std::{error::Error, fs::OpenOptions, io::Write};

#[derive(Deserialize)]
struct EdgeAccuracy {
    accuracy: CurveAccuracy,
}
#[derive(Deserialize)]
struct Snapshot {
    brep: Brep,
    edges: Vec<EdgeAccuracy>,
}
#[derive(Deserialize)]
struct Request {
    id: String,
    view: String,
    track: usize,
    face: usize,
    parameter: f64,
}
fn read<T: DeserializeOwned>(path: &str, cap: u64) -> Result<T, Box<dyn Error>> {
    let f = std::fs::File::open(path)?;
    if f.metadata()?.len() > cap {
        return Err("diagnostic input byte limit".into());
    }
    Ok(serde_json::from_reader(f)?)
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let source = args.get(1).ok_or("usage: native_silhouette_probe snapshot.json requests.json new-output.json")?;
    let requests = args.get(2).ok_or("missing requests")?;
    let output = args.get(3).ok_or("missing output")?;
    let snapshot: Snapshot = read(source, 16 * 1024 * 1024)?;
    let requests: Vec<Request> = read(requests, 256 * 1024)?;
    if requests.is_empty() || requests.len() > 512 {
        return Err("require 1 to 512 samples".into());
    }
    let first = requests.first().ok_or("missing first request")?;
    let direction = match first.view.as_str() {
        "main" => -Vec3::Z,
        "a_end" => Vec3::X,
        "g_end" => -Vec3::X,
        _ => return Err("unknown diagnostic view".into()),
    };
    if requests.iter().any(|r| r.view != first.view) {
        return Err("one view per diagnostic batch".into());
    }
    let view = hlr::View::new(direction, Vec3::Y)?;
    let start = std::time::Instant::now();
    let cancel = || start.elapsed().as_secs() >= 180;
    let opts = hlr::Options { trim_samples: 1024, max_tracks: 10_000, max_rays: 2_000_000, ..Default::default() };
    let accuracy: Vec<_> = snapshot.edges.iter().map(|e| e.accuracy).collect();
    let prepared = hlr::prepare_with_accuracy(&snapshot.brep, Some(&accuracy), &opts, &cancel)?;
    let parameters: Vec<_> =
        requests.iter().map(|r| hlr::inspect::SilhouetteParameter { track: r.track, face: r.face, parameter: r.parameter }).collect();
    let observed = hlr::inspect::silhouette_visibility(&prepared, &parameters, &view, &opts, &cancel)?;
    let probes: Vec<_> = requests.iter().zip(observed).map(|(r, p)| json!({"id":r.id,"view":r.view,"probe":p})).collect();
    let result = json!({"source":source,"trim_samples":opts.trim_samples,"report":prepared.report,
        "elapsed_seconds":start.elapsed().as_secs_f64(),"probes":probes,
        "scope":"Source-track indices belong only to this identified snapshot/view/options; candidate supports are not unique engineering ownership"});
    let mut f = OpenOptions::new().write(true).create_new(true).open(output)?;
    serde_json::to_writer(&mut f, &result)?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    println!("{} silhouette samples in {:.3} seconds", probes.len(), start.elapsed().as_secs_f64());
    Ok(())
}
