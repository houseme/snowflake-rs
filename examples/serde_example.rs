#![allow(missing_docs)]

use snowflake_me::{Snowflake, SnowflakeId, SnowflakeIdString};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    let id = sf.next_id()?;

    // Serialize as u64
    let json = serde_json::to_string(&id)?;
    println!("SnowflakeId as JSON:      {json}");
    let deserialized: SnowflakeId = serde_json::from_str(&json)?;
    assert_eq!(id, deserialized);

    // Serialize as string (JavaScript-safe)
    let string_id = SnowflakeIdString(id);
    let json = serde_json::to_string(&string_id)?;
    println!("SnowflakeIdString as JSON: {json}");
    let deserialized: SnowflakeIdString = serde_json::from_str(&json)?;
    assert_eq!(id, deserialized.0);

    // Serialize decomposed
    let decomposed = sf.decompose(id);
    let json = serde_json::to_string_pretty(&decomposed)?;
    println!("Decomposed as JSON:\n{json}");

    Ok(())
}
