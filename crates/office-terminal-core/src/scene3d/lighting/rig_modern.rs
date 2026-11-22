//! 15 modern 3-D light rig definitions.
//!
//! Port of `packages/core/src/shape/bevel-light-rigs.ts`.

use super::{LightDef, RigDefinition};

static LIGHTS_THREE_PT: &[LightDef] = &[
    LightDef {
        rgb: [1.141, 1.141, 1.141],
        direction: [-0.6515, -0.2693, -0.7093],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.5, 0.5, 0.5],
        direction: [0.8482, 0.2469, -0.4686],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [1.0, 1.0, 1.0],
        direction: [0.5634, -0.2812, 0.7769],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
];
pub static RIG_THREE_PT: RigDefinition = RigDefinition {
    ambient: 0.0,
    lights: LIGHTS_THREE_PT,
};

static LIGHTS_BALANCED: &[LightDef] = &[
    LightDef {
        rgb: [1.05, 1.05, 1.05],
        direction: [0.5263, -0.4092, -0.7453],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [1.0, 1.0, 1.0],
        direction: [-0.9386, 0.3426, -0.041],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.5, 0.5, 0.5],
        direction: [0.0934, 0.763, 0.6396],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
];
pub static RIG_BALANCED: RigDefinition = RigDefinition {
    ambient: 0.13,
    lights: LIGHTS_BALANCED,
};

static LIGHTS_SOFT: &[LightDef] = &[
    LightDef {
        rgb: [0.8, 0.8, 0.8],
        direction: [-0.6897, 0.2484, -0.6802],
        scale: 0.5,
        offset: 0.5,
        specular: true,
        diffuse: true,
    },
];
pub static RIG_SOFT: RigDefinition = RigDefinition {
    ambient: 0.3,
    lights: LIGHTS_SOFT,
};

static LIGHTS_HARSH: &[LightDef] = &[
    LightDef {
        rgb: [0.88, 0.88, 0.88],
        direction: [0.6689, -0.6755, -0.3104],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.88, 0.88, 0.88],
        direction: [-0.592, -0.7371, -0.326],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
];
pub static RIG_HARSH: RigDefinition = RigDefinition {
    ambient: 0.28,
    lights: LIGHTS_HARSH,
};

static LIGHTS_FLOOD: &[LightDef] = &[
    LightDef {
        rgb: [1.1, 1.1, 1.1],
        direction: [0.5685, -0.7651, -0.3022],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [1.1, 1.1, 1.1],
        direction: [-0.2366, -0.9595, -0.1531],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.55, 0.55, 0.55],
        direction: [-0.8982, 0.1386, -0.4171],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
];
pub static RIG_FLOOD: RigDefinition = RigDefinition {
    ambient: 0.13,
    lights: LIGHTS_FLOOD,
};

static LIGHTS_CONTRASTING: &[LightDef] = &[
    LightDef {
        rgb: [1.0, 1.0, 1.0],
        direction: [0.0, -1.0, 0.0],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: false,
    },
    LightDef {
        rgb: [1.0, 1.0, 1.0],
        direction: [0.0, 1.0, 0.0],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: false,
    },
];
pub static RIG_CONTRASTING: RigDefinition = RigDefinition {
    ambient: 1.0,
    lights: LIGHTS_CONTRASTING,
};

static LIGHTS_MORNING: &[LightDef] = &[
    LightDef {
        rgb: [0.669, 0.648, 0.596],
        direction: [0.6574, -0.7316, -0.1806],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.459, 0.454, 0.385],
        direction: [-0.2781, -0.4509, -0.8482],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
    LightDef {
        rgb: [0.9, 0.86, 0.83],
        direction: [0.672, -0.6185, -0.4073],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
    LightDef {
        rgb: [0.911, 0.846, 0.728],
        direction: [-0.1825, 0.968, 0.1722],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
];
pub static RIG_MORNING: RigDefinition = RigDefinition {
    ambient: 0.0,
    lights: LIGHTS_MORNING,
};

static LIGHTS_SUNRISE: &[LightDef] = &[
    LightDef {
        rgb: [0.667, 0.63, 0.527],
        direction: [0.6574, -0.7316, -0.1806],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.459, 0.459, 0.371],
        direction: [-0.2781, -0.4509, -0.8482],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
    LightDef {
        rgb: [0.826, 0.712, 0.638],
        direction: [0.672, -0.6185, -0.4073],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
    LightDef {
        rgb: [1.511, 1.319, 0.994],
        direction: [-0.1825, 0.968, 0.1722],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_SUNRISE: RigDefinition = RigDefinition {
    ambient: 0.0,
    lights: LIGHTS_SUNRISE,
};

static LIGHTS_SUNSET: &[LightDef] = &[
    LightDef {
        rgb: [0.672, 0.169, 0.169],
        direction: [0.6574, -0.7316, -0.1806],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.459, 0.448, 0.327],
        direction: [0.0922, -0.3551, -0.9303],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
    LightDef {
        rgb: [0.775, 0.612, 0.502],
        direction: [0.672, -0.6185, -0.4073],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
    LightDef {
        rgb: [0.761, 0.69, 0.397],
        direction: [-0.424, 0.8891, 0.1722],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_SUNSET: RigDefinition = RigDefinition {
    ambient: 0.0,
    lights: LIGHTS_SUNSET,
};

static LIGHTS_CHILLY: &[LightDef] = &[
    LightDef {
        rgb: [0.31, 0.32, 0.32],
        direction: [0.6574, -0.7316, -0.1806],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.45, 0.45, 0.45],
        direction: [-0.3539, -0.1505, -0.9231],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
    LightDef {
        rgb: [1.03, 1.02, 1.15],
        direction: [0.672, -0.6185, -0.4073],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
    LightDef {
        rgb: [0.41, 0.45, 0.48],
        direction: [-0.5781, 0.7976, 0.1722],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
];
pub static RIG_CHILLY: RigDefinition = RigDefinition {
    ambient: 0.11,
    lights: LIGHTS_CHILLY,
};

static LIGHTS_FREEZING: &[LightDef] = &[
    LightDef {
        rgb: [0.53, 0.567, 0.661],
        direction: [0.6574, -0.7316, -0.1806],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.37, 0.461, 0.461],
        direction: [-0.2781, -0.4509, -0.8482],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
    LightDef {
        rgb: [0.649, 0.638, 0.904],
        direction: [0.672, -0.6185, -0.4073],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
    LightDef {
        rgb: [0.971, 1.19, 1.363],
        direction: [-0.1825, 0.968, 0.1722],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
];
pub static RIG_FREEZING: RigDefinition = RigDefinition {
    ambient: 0.0,
    lights: LIGHTS_FREEZING,
};

static LIGHTS_FLAT: &[LightDef] = &[
    LightDef {
        rgb: [0.821, 0.821, 0.821],
        direction: [-0.9546, -0.1619, -0.2502],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: false,
    },
    LightDef {
        rgb: [2.072, 2.54, 2.91],
        direction: [0.0009, 0.8605, 0.5095],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: false,
    },
    LightDef {
        rgb: [3.843, 3.843, 3.843],
        direction: [0.6574, -0.7316, -0.1806],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: false,
    },
];
pub static RIG_FLAT: RigDefinition = RigDefinition {
    ambient: 1.0,
    lights: LIGHTS_FLAT,
};

static LIGHTS_TWO_PT: &[LightDef] = &[
    LightDef {
        rgb: [0.84, 0.84, 0.84],
        direction: [0.5266, -0.4089, -0.7454],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.3, 0.3, 0.3],
        direction: [-0.8983, 0.2365, -0.3704],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
];
pub static RIG_TWO_PT: RigDefinition = RigDefinition {
    ambient: 0.25,
    lights: LIGHTS_TWO_PT,
};

static LIGHTS_GLOW: &[LightDef] = &[
    LightDef {
        rgb: [1.0, 1.0, 1.0],
        direction: [0.0, -1.0, 0.0],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
    LightDef {
        rgb: [0.7, 0.7, 0.7],
        direction: [0.0, 1.0, 0.0],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: true,
    },
];
pub static RIG_GLOW: RigDefinition = RigDefinition {
    ambient: 1.0,
    lights: LIGHTS_GLOW,
};

static LIGHTS_BRIGHT_ROOM: &[LightDef] = &[
    LightDef {
        rgb: [1.0, 1.0, 1.0],
        direction: [0.0, -1.0, 0.0],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
    LightDef {
        rgb: [1.0, 1.0, 1.0],
        direction: [0.8227, -0.1882, -0.5364],
        scale: 1.0,
        offset: 0.0,
        specular: true,
        diffuse: false,
    },
    LightDef {
        rgb: [-0.5, -0.5, -0.5],
        direction: [0.0, 0.0, -1.0],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
    LightDef {
        rgb: [0.5, 0.5, 0.5],
        direction: [0.0, 1.0, 0.0],
        scale: 1.0,
        offset: 0.0,
        specular: false,
        diffuse: true,
    },
];
pub static RIG_BRIGHT_ROOM: RigDefinition = RigDefinition {
    ambient: 1.5,
    lights: LIGHTS_BRIGHT_ROOM,
};

pub fn lookup_modern_rig(name: &str) -> Option<&'static RigDefinition> {
    match name {
        "threePt" => Some(&RIG_THREE_PT),
        "balanced" => Some(&RIG_BALANCED),
        "soft" => Some(&RIG_SOFT),
        "harsh" => Some(&RIG_HARSH),
        "flood" => Some(&RIG_FLOOD),
        "contrasting" => Some(&RIG_CONTRASTING),
        "morning" => Some(&RIG_MORNING),
        "sunrise" => Some(&RIG_SUNRISE),
        "sunset" => Some(&RIG_SUNSET),
        "chilly" => Some(&RIG_CHILLY),
        "freezing" => Some(&RIG_FREEZING),
        "flat" => Some(&RIG_FLAT),
        "twoPt" => Some(&RIG_TWO_PT),
        "glow" => Some(&RIG_GLOW),
        "brightRoom" => Some(&RIG_BRIGHT_ROOM),
        _ => None,
    }
}
