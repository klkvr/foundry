use forge_doc::{ContractInheritance, DocBuilder, Inheritdoc};
use fs_extra::dir;
use std::path::PathBuf;
use tempfile::tempdir;
use walkdir::WalkDir;

fn collect_md_file_paths(path: &PathBuf) -> Vec<PathBuf> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().map(|ext| ext == "md").unwrap_or(false))
        .collect()
}

fn test_directory(base_name: &str) {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let sources_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join(base_name).join("src");
    let expected_docs_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join(base_name).join("docs");

    dir::copy(sources_path, root, &dir::CopyOptions::new()).unwrap();

    let builder = DocBuilder::new(root.into(), root.join("src"))
        .with_preprocessor(ContractInheritance::default())
        .with_preprocessor(Inheritdoc::default());

    let out_dir = builder.out_dir();

    builder.build().unwrap();

    let mut expected_paths = collect_md_file_paths(&expected_docs_path);
    let mut resulted_paths = collect_md_file_paths(&out_dir);

    expected_paths.sort();
    resulted_paths.sort();

    assert_eq!(expected_paths.len(), resulted_paths.len());

    for (a, b) in expected_paths.iter().zip(resulted_paths.iter()) {
        let a = std::fs::read_to_string(a).unwrap();
        let b = std::fs::read_to_string(b).unwrap();

        assert_eq!(a, b);
    }
}

macro_rules! test_directories {
    ($($dir:ident),+ $(,)?) => {$(
        #[allow(non_snake_case)]
        #[test]
        fn $dir() {
            test_directory(stringify!($dir));
        }
    )+};
}

test_directories! {
    SingleContract,
    SeparateEntries,
    Inheritdoc,
}
