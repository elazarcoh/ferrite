mod integration {
    include!("integration/test_sprite_parsing.rs");
}

mod animation {
    include!("integration/test_animation.rs");
}

mod behavior {
    include!("integration/test_behavior.rs");
}

mod window_creation {
    include!("integration/test_window_creation.rs");
}

mod config_roundtrip {
    include!("integration/test_config_roundtrip.rs");
}

mod hot_reload {
    include!("integration/test_hot_reload.rs");
}

mod sprite_gallery {
    include!("integration/test_sprite_gallery.rs");
}

mod sprite_editor {
    include!("integration/test_sprite_editor.rs");
}

mod stress {
    include!("integration/test_stress.rs");
}
