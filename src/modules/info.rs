#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModuleCategory {
    Combat,
    Movement,
    World,
    Render,
    Player,
    Misc,
    Internal,
}

impl ModuleCategory {
    pub fn from_label(label: &str) -> ModuleCategory {
        match label.to_ascii_lowercase().as_str() {
            "combat" => ModuleCategory::Combat,
            "movement" => ModuleCategory::Movement,
            "world" => ModuleCategory::World,
            "render" => ModuleCategory::Render,
            "player" => ModuleCategory::Player,
            _ => ModuleCategory::Misc,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            ModuleCategory::Combat => "Combat",
            ModuleCategory::Movement => "Movement",
            ModuleCategory::World => "World",
            ModuleCategory::Render => "Render",
            ModuleCategory::Player => "Player",
            ModuleCategory::Misc => "Misc",
            ModuleCategory::Internal => "Internal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub category: ModuleCategory,
    pub keybind: u32,
    pub eventFlags: u32,
}

impl ModuleInfo {
    pub const fn new(
        name: &'static str,
        description: &'static str,
        category: ModuleCategory,
        keybind: u32,
        eventFlags: u32,
    ) -> ModuleInfo {
        ModuleInfo {
            name,
            description,
            category,
            keybind,
            eventFlags,
        }
    }
}
