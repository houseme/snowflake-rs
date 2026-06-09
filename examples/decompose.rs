#![allow(missing_docs)]

use snowflake_me::Snowflake;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(15))
        .data_center_id(&|| Ok(7))
        .finalize()?;

    let id = sf.next_id()?;
    let decomposed = sf.decompose(id);

    println!("ID:            {}", decomposed.id);
    println!("Time (ms):     {}", decomposed.time);
    println!("Data Center:   {}", decomposed.data_center_id);
    println!("Machine:       {}", decomposed.machine_id);
    println!("Sequence:      {}", decomposed.sequence);
    println!();
    println!("Hex:           {}", decomposed.hex());
    println!("Base32:        {}", decomposed.base32());
    println!("Base36:        {}", decomposed.base36());
    println!("Base58:        {}", decomposed.base58());
    println!("Base64:        {}", decomposed.base64());
    Ok(())
}
