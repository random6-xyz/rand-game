mod bot;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    bot::run_sample_bot()?;
    Ok(())
}
