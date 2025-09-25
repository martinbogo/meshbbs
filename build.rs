use std::env;
use std::path::{Path, PathBuf};
use std::fs;

fn main() {
    // Only run if feature is enabled
    if std::env::var("CARGO_FEATURE_MESHTASTIC_PROTO").is_err() {
        return;
    }

    println!("cargo:rerun-if-env-changed=MESHTASTIC_PROTO_DIR");
    println!("cargo:rerun-if-changed=protos");

    let proto_dir = env::var("MESHTASTIC_PROTO_DIR").unwrap_or_else(|_| "protos".into());
    let proto_path = PathBuf::from(&proto_dir); // original requested path ("protos" by default)
    let mut active_proto_root = proto_path.clone(); // directory that contains either protos or the meshtastic dir

    let mut protos = Vec::new();

    fn collect_protos(dir: &Path, acc: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_protos(&path, acc);
                } else if path.extension().and_then(|e| e.to_str()) == Some("proto") {
                    acc.push(path);
                }
            }
        }
    }

    collect_protos(&proto_path, &mut protos);

    // Determine if we should attempt fallback to submodule protos.
    let mut placeholder_only = false;
    if protos.len() == 1 {
        if let Some(fname) = protos[0].file_name().and_then(|f| f.to_str()) {
            if fname == "meshtastic_placeholder.proto" { placeholder_only = true; }
        }
    }

    if protos.is_empty() || placeholder_only {
        let fallback_dir = PathBuf::from("third_party/meshtastic-protobufs/meshtastic");
        if fallback_dir.exists() {
            let mut fallback_protos = Vec::new();
            collect_protos(&fallback_dir, &mut fallback_protos);
            if !fallback_protos.is_empty() {
                eprintln!("build.rs: using Meshtastic submodule protos from '{}' (found {} files)", fallback_dir.display(), fallback_protos.len());
                protos = fallback_protos;
                active_proto_root = fallback_dir; // switch active root so include path logic below is correct
            }
        }
    }

    if protos.is_empty() {
        // Provide a fallback dummy proto to allow build to succeed. Use the
        // same package name (meshtastic) that the real protos use so the rest
        // of the code can consistently include the generated file
        // `meshtastic.rs`.
    let fallback = proto_path.join("meshtastic_placeholder.proto");
    std::fs::create_dir_all(&proto_path).expect("create proto dir");
        std::fs::write(
            &fallback,
            b"syntax = \"proto3\"; package meshtastic; message Placeholder { string note = 1; }",
        )
        .expect("write placeholder proto");
        protos.push(fallback);
    }

    // Determine include paths. If the provided directory's final component is
    // `meshtastic`, we only add its parent as the include path so imports like
    // `meshtastic/mesh.proto` resolve to that subdirectory exactly once.
    // Providing BOTH the meshtastic dir and its parent confuses protoc: the
    // same physical file becomes visible as both `channel.proto` and
    // `meshtastic/channel.proto`, leading to the duplicate definition errors
    // we observed earlier. If the directory is NOT named meshtastic (e.g. a
    // custom staging area), include it directly.
    let mut include_paths: Vec<PathBuf> = Vec::new();
    if active_proto_root.file_name().and_then(|n| n.to_str()) == Some("meshtastic") {
        if let Some(parent) = active_proto_root.parent() {
            include_paths.push(parent.to_path_buf());
        } else {
            include_paths.push(active_proto_root.clone());
        }
    } else {
        include_paths.push(active_proto_root.clone());
    }
    let includes: Vec<&Path> = include_paths.iter().map(|p| p.as_path()).collect();
    eprintln!("build.rs: Using include paths: {:?}", include_paths);

    let mut config = prost_build::Config::new();
    config.bytes(["."]);

    // Workaround: The Meshtastic repo has many protos all within the same package
    // and some .proto files import others using the fully qualified path prefix
    // (meshtastic/...). We compile them in a single pass. Prost (and protoc) can
    // emit redefinition errors if a file is passed twice. Ensure we pass each
    // proto only once by sorting and deduping.
    let mut unique = protos.clone();
    unique.sort();
    unique.dedup();

    eprintln!("build.rs: Compiling {} proto files", unique.len());
    for p in &unique { eprintln!("  proto: {}", p.display()); }

    config
        .compile_protos(&unique, &includes)
        .expect("Failed to compile protos");
}