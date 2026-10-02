//! Regression coverage for configuration validation and injected clock setup.

use snowflake_me::{Builder, ClockDriftStrategy, Error, Snowflake};

fn builder() -> Builder<'static> {
    Snowflake::builder()
        .start_time(0)
        .clock_source(|| 0)
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
}

#[test]
fn invalid_bit_layouts_return_errors_without_overflowing() {
    let invalid_layouts = [
        (255, 54, 5, 5), // The old u8 sum wrapped to 63 in release builds.
        (255, 255, 255, 255),
        (64, 0, 0, 0),
        (0, 64, 0, 0),
        (41, 12, 5, 6),
        (0, 0, 0, 0),
        (30, 0, 17, 16), // Node IDs are represented as u16.
        (30, 0, 16, 17),
    ];

    for (time, sequence, data_center, machine) in invalid_layouts {
        let result = builder()
            .bit_len_time(time)
            .bit_len_sequence(sequence)
            .bit_len_data_center_id(data_center)
            .bit_len_machine_id(machine)
            .finalize();

        assert!(
            matches!(result, Err(Error::InvalidBitLength(t, s, d, m))
                if (t, s, d, m) == (time, sequence, data_center, machine)),
            "invalid layout {time}/{sequence}/{data_center}/{machine} was not rejected"
        );
    }
}

#[test]
fn sixteen_bit_node_ids_preserve_the_full_u16_range() {
    let sf = builder()
        .bit_len_time(30)
        .bit_len_sequence(1)
        .bit_len_data_center_id(16)
        .bit_len_machine_id(16)
        .machine_id(&|| Ok(u16::MAX))
        .data_center_id(&|| Ok(u16::MAX))
        .clock_source(|| 1)
        .finalize()
        .unwrap();

    let parts = sf.decompose(sf.next_id().unwrap());
    assert_eq!(
        (
            parts.time,
            parts.sequence,
            parts.data_center_id,
            parts.machine_id
        ),
        (1, 0, u64::from(u16::MAX), u64::from(u16::MAX))
    );
}

#[test]
fn maximum_time_width_can_generate_at_the_epoch() {
    let sf = builder()
        .bit_len_time(63)
        .bit_len_sequence(0)
        .bit_len_data_center_id(0)
        .bit_len_machine_id(0)
        .finalize()
        .unwrap();

    assert_eq!(sf.next_id().unwrap().as_u64(), 0);
}

#[test]
fn maximum_sequence_width_can_generate_at_the_epoch() {
    let sf = builder()
        .bit_len_time(0)
        .bit_len_sequence(63)
        .bit_len_data_center_id(0)
        .bit_len_machine_id(0)
        .finalize()
        .unwrap();

    let ids = sf.next_ids(2).unwrap();
    assert_eq!((ids[0].as_u64(), ids[1].as_u64()), (0, 1));
}

#[test]
fn zero_width_machine_id_rejects_nonzero_values() {
    let result = builder()
        .bit_len_machine_id(0)
        .bit_len_sequence(17)
        .machine_id(&|| Ok(1))
        .finalize();

    assert!(matches!(result, Err(Error::MachineIdFailed(_))));
}

#[test]
fn zero_width_data_center_id_rejects_nonzero_values() {
    let result = builder()
        .bit_len_data_center_id(0)
        .bit_len_sequence(17)
        .data_center_id(&|| Ok(1))
        .finalize();

    assert!(matches!(result, Err(Error::DataCenterIdFailed(_))));
}

#[test]
fn negative_clock_drift_limit_is_rejected_for_every_strategy() {
    for strategy in [
        ClockDriftStrategy::Wait,
        ClockDriftStrategy::Error,
        ClockDriftStrategy::LastTimestamp,
    ] {
        let result = builder()
            .clock_drift_strategy(strategy)
            .max_clock_drift_ms(-1)
            .finalize();

        assert!(matches!(result, Err(Error::InvalidClockDrift(-1))));
    }
}

#[test]
fn zero_clock_drift_limit_is_accepted() {
    assert!(builder().max_clock_drift_ms(0).finalize().is_ok());
}

#[test]
fn start_time_validation_uses_the_custom_clock() {
    // A deterministic clock may use an epoch far beyond the system clock.
    let sf = builder()
        .start_time(i64::MAX)
        .clock_source(|| i64::MAX)
        .finalize()
        .unwrap();

    assert_eq!(sf.next_id().unwrap().as_u64(), 0);
}

#[cfg(feature = "std")]
#[test]
fn start_time_ahead_of_the_custom_clock_is_rejected() {
    let result = builder().start_time(1).clock_source(|| 0).finalize();

    assert!(matches!(result, Err(Error::StartTimeAheadOfCurrentTime(1))));
}
