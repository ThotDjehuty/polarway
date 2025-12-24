fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protocol buffers
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile(
            &["../proto/polaroid.proto"],
            &["../proto"],
        )?;
    
    println!("cargo:rerun-if-changed=../proto/polaroid.proto");
    
    Ok(())
}
