// Integration test: Note creation and retrieval round-trip
// Tests that notes can be created, stored, and retrieved correctly through the Storage trait

use joplin_domain::{Folder, Note, Storage};
use neojoplin_storage::SqliteStorage;

#[tokio::test]
async fn test_note_roundtrip() {
    // Use default storage path (in-memory or temp)
    let storage = SqliteStorage::new()
        .await
        .expect("Failed to create storage");
    
    // Create a folder using Default + set required fields
    let mut folder = Folder::default();
    folder.id = "test-folder-roundtrip".to_string();
    folder.title = "Test Folder".to_string();
    
    storage
        .create_folder(&folder)
        .await
        .expect("Failed to create folder");
    
    // Create a note using Default + set required fields
    let mut note = Note::default();
    note.id = "test-note-roundtrip".to_string();
    note.title = "Test Note Roundtrip".to_string();
    note.body = "This is a test note body for roundtrip testing".to_string();
    note.parent_id = folder.id.clone();
    
    storage
        .create_note(&note)
        .await
        .expect("Failed to create note");
    
    // Retrieve the note
    let retrieved = storage
        .get_note("test-note-roundtrip")
        .await
        .expect("Failed to retrieve note")
        .expect("Note should exist");
    
    assert_eq!(retrieved.id, note.id);
    assert_eq!(retrieved.title, note.title);
    assert_eq!(retrieved.body, note.body);
    assert_eq!(retrieved.parent_id, note.parent_id);
    
    // List all notes
    let all_notes = storage
        .list_notes(None)
        .await
        .expect("Failed to list notes");
    
    assert!(all_notes.len() >= 1, "Expected at least 1 note");
    assert!(all_notes.iter().any(|n| n.id == note.id), "Note not found in list");
    
    // Update the note
    let mut updated_note = note.clone();
    updated_note.title = "Updated Test Note".to_string();
    updated_note.body = "Updated body content".to_string();
    
    storage
        .update_note(&updated_note)
        .await
        .expect("Failed to update note");
    
    // Verify the update
    let retrieved_updated = storage
        .get_note("test-note-roundtrip")
        .await
        .expect("Failed to retrieve updated note")
        .expect("Updated note should exist");
    
    assert_eq!(retrieved_updated.title, "Updated Test Note");
    assert_eq!(retrieved_updated.body, "Updated body content");
    
    // Cleanup
    storage
        .delete_note(&note.id)
        .await
        .expect("Failed to delete note");
    storage
        .delete_folder(&folder.id)
        .await
        .expect("Failed to delete folder");
}
