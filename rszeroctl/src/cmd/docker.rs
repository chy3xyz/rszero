use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn execute(main_file: &str, output: &str) -> Result<()> {
    let binary_name = Path::new(main_file)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    let dockerfile = format!(
        r#"# Build stage
FROM rust:1.75-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY . .
RUN cargo build --release --bin {binary_name}

# Runtime stage
FROM alpine:3.19
RUN apk --no-cache add ca-certificates
WORKDIR /root/
COPY --from=builder /app/target/release/{binary_name} .
EXPOSE 8080
CMD ["./{binary_name}"]
"#
    );

    fs::write(output, dockerfile)?;
    println!("  Generated Dockerfile: {}", output);
    Ok(())
}
