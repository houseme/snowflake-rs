#![allow(missing_docs)]

use snowflake_me::{ClockDriftStrategy, Snowflake};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(10))
        .data_center_id(&|| Ok(5))
        .clock_drift_strategy(ClockDriftStrategy::Wait)
        .max_clock_drift_ms(5000)
        .bit_len_time(41)
        .bit_len_sequence(12)
        .bit_len_data_center_id(5)
        .bit_len_machine_id(5)
        .finalize()?;

    let id = sf.next_id()?;
    println!("Generated ID: {id}");
    Ok(())
}
