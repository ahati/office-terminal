//! 12 legacy 3-D light rig definitions.
//!
//! Port of `packages/core/src/shape/bevel-light-rigs.ts`.

use super::{LightDef, RigDefinition};

static LIGHTS_LEGACY_FLAT1: &[LightDef] = &[
    LightDef {
        rgb: [0.58, 0.58, 0.58],
        direction: [0.0, 0.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.58, 0.58, 0.58],
        direction: [0.0, 0.0, -0.2],
        scale: 0.5,
        offset: 0.5,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_LEGACY_FLAT1: RigDefinition = RigDefinition {
    ambient: 0.305,
    lights: LIGHTS_LEGACY_FLAT1,
};

static LIGHTS_LEGACY_FLAT2: &[LightDef] = &[
    LightDef {
        rgb: [0.58, 0.58, 0.58],
        direction: [-1.0, -1.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.58, 0.58, 0.58],
        direction: [0.0, 1.0, -0.2],
        scale: 0.5,
        offset: 0.5,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_LEGACY_FLAT2: RigDefinition = RigDefinition {
    ambient: 0.305,
    lights: LIGHTS_LEGACY_FLAT2,
};

static LIGHTS_LEGACY_FLAT3: &[LightDef] = &[
    LightDef {
        rgb: [0.58, 0.58, 0.58],
        direction: [0.0, -1.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.58, 0.58, 0.58],
        direction: [0.0, 1.0, -0.2],
        scale: 0.5,
        offset: 0.5,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_LEGACY_FLAT3: RigDefinition = RigDefinition {
    ambient: 0.305,
    lights: LIGHTS_LEGACY_FLAT3,
};

static LIGHTS_LEGACY_FLAT4: &[LightDef] = &[
    LightDef {
        rgb: [0.58, 0.58, 0.58],
        direction: [1.0, -1.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.58, 0.58, 0.58],
        direction: [0.0, 1.0, -0.2],
        scale: 0.5,
        offset: 0.5,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_LEGACY_FLAT4: RigDefinition = RigDefinition {
    ambient: 0.305,
    lights: LIGHTS_LEGACY_FLAT4,
};

static LIGHTS_LEGACY_NORMAL1: &[LightDef] = &[
    LightDef {
        rgb: [0.671, 0.671, 0.671],
        direction: [0.0, 0.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.366, 0.366, 0.366],
        direction: [0.0, 0.0, -0.2],
        scale: 0.5,
        offset: 0.5,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_LEGACY_NORMAL1: RigDefinition = RigDefinition {
    ambient: 0.153,
    lights: LIGHTS_LEGACY_NORMAL1,
};

static LIGHTS_LEGACY_NORMAL2: &[LightDef] = &[
    LightDef {
        rgb: [0.671, 0.671, 0.671],
        direction: [-1.0, -1.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.366, 0.366, 0.366],
        direction: [0.0, 1.0, -0.2],
        scale: 0.5,
        offset: 0.5,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_LEGACY_NORMAL2: RigDefinition = RigDefinition {
    ambient: 0.153,
    lights: LIGHTS_LEGACY_NORMAL2,
};

static LIGHTS_LEGACY_NORMAL3: &[LightDef] = &[
    LightDef {
        rgb: [0.671, 0.671, 0.671],
        direction: [0.0, -1.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.366, 0.366, 0.366],
        direction: [0.0, 1.0, -0.2],
        scale: 0.5,
        offset: 0.5,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_LEGACY_NORMAL3: RigDefinition = RigDefinition {
    ambient: 0.153,
    lights: LIGHTS_LEGACY_NORMAL3,
};

static LIGHTS_LEGACY_NORMAL4: &[LightDef] = &[
    LightDef {
        rgb: [0.671, 0.671, 0.671],
        direction: [1.0, -1.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.366, 0.366, 0.366],
        direction: [0.0, 1.0, -0.2],
        scale: 0.5,
        offset: 0.5,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_LEGACY_NORMAL4: RigDefinition = RigDefinition {
    ambient: 0.153,
    lights: LIGHTS_LEGACY_NORMAL4,
};

static LIGHTS_LEGACY_HARSH1: &[LightDef] = &[
    LightDef {
        rgb: [0.793, 0.793, 0.793],
        direction: [0.0, 0.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.214, 0.214, 0.214],
        direction: [0.0, 0.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_LEGACY_HARSH1: RigDefinition = RigDefinition {
    ambient: 0.061,
    lights: LIGHTS_LEGACY_HARSH1,
};

static LIGHTS_LEGACY_HARSH2: &[LightDef] = &[
    LightDef {
        rgb: [0.793, 0.793, 0.793],
        direction: [-1.0, -1.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.214, 0.214, 0.214],
        direction: [0.0, 1.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_LEGACY_HARSH2: RigDefinition = RigDefinition {
    ambient: 0.061,
    lights: LIGHTS_LEGACY_HARSH2,
};

static LIGHTS_LEGACY_HARSH3: &[LightDef] = &[
    LightDef {
        rgb: [0.793, 0.793, 0.793],
        direction: [0.0, -1.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.214, 0.214, 0.214],
        direction: [0.0, 1.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_LEGACY_HARSH3: RigDefinition = RigDefinition {
    ambient: 0.061,
    lights: LIGHTS_LEGACY_HARSH3,
};

static LIGHTS_LEGACY_HARSH4: &[LightDef] = &[
    LightDef {
        rgb: [0.793, 0.793, 0.793],
        direction: [1.0, -1.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.214, 0.214, 0.214],
        direction: [0.0, 1.0, -0.2],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_LEGACY_HARSH4: RigDefinition = RigDefinition {
    ambient: 0.061,
    lights: LIGHTS_LEGACY_HARSH4,
};

pub fn lookup_legacy_rig(name: &str) -> Option<&'static RigDefinition> {
    match name {
        "legacyFlat1" => Some(&RIG_LEGACY_FLAT1),
        "legacyFlat2" => Some(&RIG_LEGACY_FLAT2),
        "legacyFlat3" => Some(&RIG_LEGACY_FLAT3),
        "legacyFlat4" => Some(&RIG_LEGACY_FLAT4),
        "legacyNormal1" => Some(&RIG_LEGACY_NORMAL1),
        "legacyNormal2" => Some(&RIG_LEGACY_NORMAL2),
        "legacyNormal3" => Some(&RIG_LEGACY_NORMAL3),
        "legacyNormal4" => Some(&RIG_LEGACY_NORMAL4),
        "legacyHarsh1" => Some(&RIG_LEGACY_HARSH1),
        "legacyHarsh2" => Some(&RIG_LEGACY_HARSH2),
        "legacyHarsh3" => Some(&RIG_LEGACY_HARSH3),
        "legacyHarsh4" => Some(&RIG_LEGACY_HARSH4),
        _ => None,
    }
}
