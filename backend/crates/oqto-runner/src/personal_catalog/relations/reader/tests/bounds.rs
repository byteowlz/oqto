use super::*;

#[tokio::test]
async fn incomplete_owner_binding_set_cannot_hide_a_late_conflicting_author() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    fixture.write(1, vec![fixture.record(1, 2)])?;
    let subset = vec![fixture.bindings[0].clone(), fixture.bindings[2].clone()];
    let mut output = fixture.data.clone();
    super::super::super::enrich(
        &mut output,
        Some(Context {
            source: &fixture.source,
            bindings: &subset,
        }),
    )
    .await;
    empty(&output);
    assert_eq!(output, fixture.data);
    Ok(())
}

#[tokio::test]
async fn exact_file_byte_and_raw_record_limits_not_just_output_edge_count() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    // Duplicate known kinds are deliberately invalid in the shared codec.
    // Use one valid fact +199 unknown records to isolate the raw-input bound.
    let mut records = vec![json!({"kind":"unknown"}); 199];
    records.push(fixture.record(0, 2));
    let envelope = fixture.envelope(0, records.clone());
    let mut bytes = serde_json::to_vec(&envelope)?;
    bytes.resize(64 * 1024, b' ');
    std::fs::write(fixture.sidecar(0), &bytes)?;
    fixture.expect(&fixture.output().await, 2, 0);
    bytes.push(b' ');
    std::fs::write(fixture.sidecar(0), &bytes)?;
    empty(&fixture.output().await);
    records.push(json!({"kind":"unknown"}));
    fixture.write(0, records)?;
    empty(&fixture.output().await);
    Ok(())
}

#[tokio::test]
async fn global_file_byte_edge_candidate_and_deadline_bounds_never_admit_prefix() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    fixture.write(1, vec![json!({"kind":"unknown"})])?;
    // This same complete input MUST be positive; otherwise an empty result
    // would not prove any work bound (e.g. conflicts could mask a broken cap).
    fixture.expect(&fixture.output().await, 2, 0);
    let size =
        std::fs::metadata(fixture.sidecar(0))?.len() + std::fs::metadata(fixture.sidecar(1))?.len();
    for limits in [
        Limits {
            files: 1,
            ..Limits::default()
        },
        Limits {
            bytes: size as usize - 1,
            ..Limits::default()
        },
        Limits {
            edges: 1,
            ..Limits::default()
        },
        Limits {
            candidates: 2,
            ..Limits::default()
        },
        Limits {
            deadline: Duration::ZERO,
            ..Limits::default()
        },
    ] {
        let output = fixture.limited(limits).await;
        empty(&output);
        assert_eq!(output, fixture.data); // budgets cannot drop/change base rows
    }
    // Fully read two conflicting parents also produce none, not first wins.
    fixture.write(1, vec![fixture.record(1, 2)])?;
    empty(&fixture.output().await);
    Ok(())
}

#[tokio::test]
async fn conflicts_cycles_and_unknown_fields_are_validated_after_full_bounded_batch() -> Result<()>
{
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 1), fixture.record(0, 2)])?;
    fixture.write(1, vec![fixture.record(1, 0)])?;
    let output = fixture.output().await;
    assert!(output["sessions"][0].get("relations").is_none());
    assert!(output["sessions"][1].get("relations").is_none());
    fixture.expect(&output, 2, 0);
    let mut envelope = fixture.envelope(0, vec![fixture.record(0, 2)]);
    envelope["unknown"] = json!({"sourceId":"never-authority","parent":"legacy"});
    std::fs::write(fixture.sidecar(0), serde_json::to_vec(&envelope)?)?;
    std::fs::remove_file(fixture.sidecar(1))?;
    fixture.expect(&fixture.output().await, 2, 0);
    Ok(())
}
