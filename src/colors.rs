/// (id, menu label, 0xRRGGBB). "default" restores the standard Windows icon.
pub const COLORS: &[(&str, &str, u32)] = &[
    ("default", "Default", 0xFFD45E),
    ("red", "Red", 0xE03C3C),
    ("orange", "Orange", 0xF28A1E),
    ("gold", "Gold", 0xD9A514),
    ("brown", "Brown", 0x8B5A2B),
    ("green", "Green", 0x2EAE3B),
    ("cyan", "Cyan", 0x2EC4B6),
    ("lightblue", "Light blue", 0x4DA3D9),
    ("blue", "Blue", 0x2447C9),
    ("pink", "Pink", 0xF58A8A),
    ("purple", "Purple", 0xC2339A),
    ("black", "Black", 0x3A3A3A),
];

/// Extra colors, only offered by the "Custom color..." palette (the menu has no room for them).
pub const EXTRA: &[(&str, &str, u32)] = &[
    ("aqua", "Aqua", 0x8FE8F0),
    ("violet", "Violet", 0x7B2FBE),
    ("grey", "Grey", 0xA8A294),
    ("lime", "Lime", 0x9BD826),
    ("teal", "Teal", 0x138A8A),
    ("mint", "Mint", 0x7FE0B0),
    ("navy", "Navy", 0x1B2A6B),
    ("magenta", "Magenta", 0xE91E8C),
    ("coral", "Coral", 0xFF6F59),
    ("maroon", "Maroon", 0x8C1C2B),
    ("lavender", "Lavender", 0xB7A6F0),
    ("white", "White", 0xF2F2F2),
];
