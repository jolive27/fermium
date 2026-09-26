//! load / table / fit against v1's output (`python3 -m fermium run tests/fixtures/data/fit_demo.fm`,
//! saved as fit_demo.out).
use fermium_runtime::data::*;

fn fx(name: &str) -> String {
    format!("{}/tests/fixtures/data/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn header_load_and_errors() {
    let h = read_csv_header(&fx("pendulum.csv")).unwrap();
    assert_eq!(h[0], HeaderCol { header: "L [cm]".into(), name: "L".into(), unit: Some("cm".into()) });
    assert_eq!(h[1].name, "T");
    let d = read_csv_header(&fx("decay.csv")).unwrap();
    assert_eq!(d[1], HeaderCol { header: "N".into(), name: "N".into(), unit: None });
    let ds = load(&fx("pendulum.csv"), "pendulum.csv", &[ColUnit { factor: 0.01, offset: 0.0 }, ColUnit { factor: 1.0, offset: 0.0 }]).unwrap();
    assert_eq!(ds.len(), 6); // the blank line is skipped
    assert_eq!(ds.cols[0][0], 0.2);
    let dir = std::env::temp_dir().join(format!("fm_data_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.csv");
    std::fs::write(&bad, "x [m],y\n1,2\n3\n").unwrap();
    let u = [ColUnit { factor: 1.0, offset: 0.0 }; 2];
    assert_eq!(load(bad.to_str().unwrap(), "bad.csv", &u).unwrap_err(), "bad.csv, line 3: expected 2 values but found 1");
    std::fs::write(&bad, "x [m],y\n1,2\n3,four\n").unwrap();
    assert_eq!(load(bad.to_str().unwrap(), "bad.csv", &u).unwrap_err(), "bad.csv, line 3: not a number: ['3', 'four']");
    assert!(load("/nonexistent/q.csv", "q.csv", &u).unwrap_err().starts_with("can't find the file 'q.csv'"));
    assert_eq!(table(vec![vec![1.0, 2.0], vec![3.0]]).unwrap_err(), "the columns of this table have different lengths (2 and 1)");
}

#[test]
fn fit_report_matches_v1() {
    let want: Vec<String> = std::fs::read_to_string(fx("fit_demo.out")).unwrap().lines().map(str::to_string).collect();
    let one = ColUnit { factor: 1.0, offset: 0.0 };
    let pen = load(&fx("pendulum.csv"), "pendulum.csv", &[ColUnit { factor: 0.01, offset: 0.0 }, one]).unwrap();
    let dec = load(&fx("decay.csv"), "decay.csv", &[ColUnit { factor: 60.0, offset: 0.0 }, one]).unwrap();
    let mut lines = Vec::new();
    // fit T = 2π √(L/g) to data
    let (l, t) = (&pen.cols[0], &pen.cols[1]);
    let mut r1 = |p: &[f64], o: &mut [f64]| {
        for i in 0..l.len() {
            o[i] = 2.0 * std::f64::consts::PI * (l[i] / p[0]).sqrt() - t[i];
        }
    };
    let info = FitInfo {
        text: "T = 2π √(L/g)".into(),
        path: "pendulum.csv".into(),
        params: vec![ParamInfo { name: "g".into(), unit: "m/s²".into(), factor: 1.0, offset: 0.0 }],
        y_unit: "s".into(),
        y_factor: 1.0,
    };
    lines.extend(run_fit(&mut r1, l.len(), &[None], &info).unwrap().lines);
    // fit N = N0 exp(-t/τ) to d
    let (tt, nn) = (&dec.cols[0], &dec.cols[1]);
    let mut r2 = |p: &[f64], o: &mut [f64]| {
        for i in 0..tt.len() {
            o[i] = p[0] * (-tt[i] / p[1]).exp() - nn[i];
        }
    };
    let info2 = FitInfo {
        text: "N = N0 exp(-t/τ)".into(),
        path: "decay.csv".into(),
        params: vec![
            ParamInfo { name: "N0".into(), unit: "".into(), factor: 1.0, offset: 0.0 },
            ParamInfo { name: "τ".into(), unit: "min".into(), factor: 60.0, offset: 0.0 },
        ],
        y_unit: "".into(),
        y_factor: 1.0,
    };
    let out2 = run_fit(&mut r2, tt.len(), &[None, None], &info2).unwrap();
    let n0 = out2.result.params[0];
    lines.extend(out2.lines);
    // fit N = N0 exp(-t/τ) + B to d with τ = 5 min   (N0 is now known)
    let mut r3 = |p: &[f64], o: &mut [f64]| {
        for i in 0..tt.len() {
            o[i] = n0 * (-tt[i] / p[0]).exp() + p[1] - nn[i];
        }
    };
    let info3 = FitInfo {
        text: "N = N0 exp(-t/τ) + B".into(),
        path: "decay.csv".into(),
        params: vec![
            ParamInfo { name: "τ".into(), unit: "min".into(), factor: 60.0, offset: 0.0 },
            ParamInfo { name: "B".into(), unit: "".into(), factor: 1.0, offset: 0.0 },
        ],
        y_unit: "".into(),
        y_factor: 1.0,
    };
    lines.extend(run_fit(&mut r3, tt.len(), &[Some(300.0), None], &info3).unwrap().lines);
    assert_eq!(lines, want);
}
