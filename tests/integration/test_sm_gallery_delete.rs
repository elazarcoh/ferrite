// Integration test: SmGallery::delete removes the file and drops the entry from the list.

use my_pet::sprite::sm_gallery::SmGallery;

fn valid_sm_toml(name: &str) -> String {
    format!(
        r#"
[meta]
name = "{name}"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"
transitions = []

[states.grabbed]
required = true
action = "grabbed"
transitions = []

[states.fall]
required = true
action = "fall"
transitions = []

[states.thrown]
required = true
action = "thrown"
transitions = []
"#,
        name = name
    )
}

#[test]
fn delete_sm_removes_file_and_entry() {
    let dir = tempfile::tempdir().unwrap();
    let mut gallery = SmGallery::load(dir.path());

    // Save a valid SM so it lands in state_machines/<name>.petstate
    gallery
        .save("DeleteMe", &valid_sm_toml("DeleteMe"))
        .expect("save must succeed");

    // Confirm it is listed
    assert!(
        gallery.valid_names().contains(&"DeleteMe"),
        "SM must appear in valid list after save"
    );

    // The file should exist on disk
    let sm_file = dir.path().join("state_machines").join("DeleteMe.petstate");
    assert!(sm_file.exists(), "petstate file must exist on disk after save");

    // Delete the SM
    gallery.delete("DeleteMe").expect("delete must succeed");

    // File must be gone
    assert!(!sm_file.exists(), "petstate file must be removed after delete");

    // Gallery must no longer list it
    assert!(
        !gallery.valid_names().contains(&"DeleteMe"),
        "SM must no longer appear in valid list after delete"
    );
}

#[test]
fn delete_draft_sm_removes_file_and_entry() {
    let dir = tempfile::tempdir().unwrap();
    let mut gallery = SmGallery::load(dir.path());

    // Save an invalid SM so it becomes a draft
    let bad_source = r#"
[meta]
name = "DraftDelete"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
action = "idle"
"#;
    gallery
        .save("DraftDelete", bad_source)
        .expect("save of draft must not return IO error");

    assert!(
        gallery.draft_names().contains(&"DraftDelete"),
        "bad SM must appear in draft list"
    );

    let draft_file = dir
        .path()
        .join("state_machines")
        .join("drafts")
        .join("DraftDelete.draft.petstate");
    assert!(draft_file.exists(), "draft file must exist after save");

    gallery.delete("DraftDelete").expect("delete draft must succeed");

    assert!(!draft_file.exists(), "draft file must be removed after delete");
    assert!(
        !gallery.draft_names().contains(&"DraftDelete"),
        "SM must no longer appear in draft list after delete"
    );
}
