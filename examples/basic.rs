#![allow(missing_docs)]

use snowflake_me::Snowflake;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    let id = sf.next_id()?;
    println!("Generated ID: {id}");
    println!("As u64: {}", id.as_u64());
    println!("As hex: {}", id.hex());
    Ok(())
}
