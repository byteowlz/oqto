use super::*;
use std::os::unix::fs::MetadataExt;

#[tokio::test]
async fn in_place_header_change_with_same_inode_still_requires_current_header_and_pin() -> Result<()>
{
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    let target = fixture.bindings[0].session_file.clone();
    let original = std::fs::metadata(&target)?;
    hook(move || {
        std::fs::write(
            &target,
            b"{\"type\":\"not_session\",\"id\":\"invalid\",\"cwd\":\"/synthetic\"}\n",
        )?;
        let after = std::fs::metadata(target)?;
        assert_eq!((original.dev(), original.ino()), (after.dev(), after.ino()));
        Ok(())
    });
    empty(&fixture.output().await);
    Ok(())
}

#[tokio::test]
async fn directory_replacement_preserving_pinned_native_file_inodes_rejects_snapshot() -> Result<()>
{
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    let directory = fixture.bindings[0]
        .session_file
        .parent()
        .unwrap()
        .to_owned();
    let binding = fixture.bindings[0].clone();
    let source = fixture.source.clone();
    hook(move || {
        let saved = directory.with_extension("held");
        std::fs::rename(&directory, &saved)?;
        std::fs::create_dir(&directory)?;
        for entry in std::fs::read_dir(&saved)? {
            let entry = entry?;
            std::fs::rename(entry.path(), directory.join(entry.file_name()))?;
        }
        // The native file inode and its actual durable source proof still match.
        // The extra held-directory snapshot must nevertheless refuse this batch.
        let header = source.header(&binding.session_file)?;
        assert_eq!(header.external_id, binding.external_session_id);
        Ok(())
    });
    empty(&fixture.output().await);
    fixture.expect(&fixture.output().await, 2, 0); // next fresh complete snapshot works
    Ok(())
}
