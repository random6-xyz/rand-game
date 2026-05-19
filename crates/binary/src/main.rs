mod flatbuffer_reader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    flatbuffer_reader::run_sample_bot()?;
    Ok(())
}
