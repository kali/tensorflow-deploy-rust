use std::{fs, path};

pub fn dir() -> path::PathBuf {
    let cache = ::std::env::var("CACHEDIR").ok().unwrap_or("../../.cached".to_string());
    fs::create_dir_all(&cache).unwrap();
    path::PathBuf::from(cache).join("onnx")
}

pub fn ensure_onnx_git_checkout() {
    println!("cargo:rerun-if-changed={}", dir().to_str().unwrap());
    use std::sync::Once;
    static START: Once = Once::new();
    START.call_once(|| {
        use fs2::FileExt;
        fs::create_dir_all(dir()).unwrap();
        let lockfile = dir().join(".lock");
        let _lock = fs::File::create(lockfile).unwrap().lock_exclusive();
        let run = std::process::Command::new("./checkout.sh").status().unwrap();
        if !run.success() {
            panic!("Failed to checkout onnx")
        }
        println!("onnx checkout done");
    });
}

pub fn make_test_file(tests_set: &str, onnx_tag: &str) {
    use std::io::Write;
    ensure_onnx_git_checkout();
    let sane_tag = onnx_tag.replace(".", "_");
    let node_tests =
        dir().join(format!("onnx-{}", onnx_tag)).join("onnx/backend/test/data").join(tests_set);
    assert!(node_tests.exists());
    let working_list_file =
        path::PathBuf::from(".").join(format!("{}-{}.txt", tests_set, onnx_tag));
    println!("cargo:rerun-if-changed={}", working_list_file.to_str().unwrap());
    let working_list: Vec<(String, bool)> = fs::read_to_string(&working_list_file)
        .unwrap()
        .split("\n")
        .map(|s| s.to_string())
        .filter(|s| s.trim().len() > 1 && s.trim().as_bytes()[0] != b'#')
        .map(|s| {
            let splits = s.split_whitespace().collect::<Vec<_>>();
            (splits[0].to_string(), splits.len() == 1)
        })
        .collect();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_dir = path::PathBuf::from(out_dir);
    let test_dir = out_dir.join("tests");
    fs::create_dir_all(&test_dir).unwrap();
    let test_file = test_dir.join(tests_set).with_extension("rs");
    let mut rs = fs::File::create(test_file).unwrap();
    let mut tests: Vec<String> = fs::read_dir(&node_tests)
        .unwrap()
        .map(|de| de.unwrap().file_name().to_str().unwrap().to_owned())
        .collect();
    tests.sort();
    writeln!(rs, "mod {}_{} {{", tests_set.replace("-", "_"), sane_tag).unwrap();
    for (s, optim) in &[("plain", false), ("optim", true)] {
        writeln!(rs, "mod {} {{", s).unwrap();
        for t in &tests {
            writeln!(rs, "#[test]").unwrap();
            let pair = working_list.iter().find(|pair| &*pair.0 == &*t);
            let run = pair.is_some() && (pair.unwrap().1 || !optim);
            if !run {
                writeln!(rs, "#[ignore]").unwrap();
            }
            writeln!(rs, "fn {}() {{", t).unwrap();
            writeln!(rs, "crate::onnx::run_one({:?}, {:?}, {:?})", node_tests, t, optim).unwrap();
            writeln!(rs, "}}").unwrap();
        }
        writeln!(rs, "}}").unwrap();
    }
    writeln!(rs, "}}").unwrap();
}

fn main() {
    ensure_onnx_git_checkout();
    for set in "node real simple pytorch-operator pytorch-converted".split_whitespace() {
        for ver in "1.4.1 1.5.0".split_whitespace() {
            make_test_file(set, ver);
        }
    }
}
