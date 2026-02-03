fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protocol buffers
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(
            &["../proto/polarway.proto"],
            &["../proto"],
        )?;
    
    println!("cargo:rerun-if-changed=../proto/polarway.proto");
    
    Ok(())
}
