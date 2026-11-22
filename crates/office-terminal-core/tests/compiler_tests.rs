use office_terminal_core::*;
use crossbeam_channel::unbounded;
use pptx_model::*;

#[test]
fn test_shape_presets_compilation() {
    let (tx, rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    let slide_json = serde_json::json!({
        "index": 0,
        "slideNumber": 1,
        "background": {
            "fillType": "solid",
            "color": "112233"
        },
        "elements": [
            {
                "type": "shape",
                "x": 100000,
                "y": 200000,
                "width": 400000,
                "height": 300000,
                "rotation": 45.0,
                "flipH": true,
                "flipV": false,
                "geometry": "roundRect",
                "fill": {
                    "fillType": "solid",
                    "color": "00FF00"
                },
                "stroke": {
                    "color": "FF0000",
                    "width": 12700,
                    "dashStyle": "dash",
                    "lineCap": "round",
                    "lineJoin": "round"
                },
                "adj": 20000.0
            },
            {
                "type": "shape",
                "x": 500000,
                "y": 500000,
                "width": 300000,
                "height": 150000,
                "rotation": 0.0,
                "flipH": false,
                "flipV": false,
                "geometry": "rightArrow",
                "fill": {
                    "fillType": "gradient",
                    "angle": 90.0,
                    "gradType": "linear",
                    "stops": [
                        { "position": 0.0, "color": "0000FF" },
                        { "position": 1.0, "color": "00FFFF" }
                    ]
                },
                "adj": 40000.0,
                "adj2": 60000.0
            },
            {
                "type": "shape",
                "x": 0,
                "y": 0,
                "width": 100000,
                "height": 100000,
                "rotation": 0.0,
                "flipH": false,
                "flipV": false,
                "geometry": "custGeom",
                "fill": {
                    "fillType": "solid",
                    "color": "123456"
                },
                "custGeom": [
                    [
                        { "cmd": "moveTo", "x": 0.0, "y": 0.0 },
                        { "cmd": "lineTo", "x": 0.5, "y": 1.0 },
                        { "cmd": "lineTo", "x": 1.0, "y": 0.0 },
                        { "cmd": "close" }
                    ]
                ]
            }
        ],
        "elementSources": [],
        "comments": []
    });

    let slide: Slide = serde_json::from_value(slide_json).expect("deserialize slide");

    let vector_slide = compiler
        .compile(&slide, 9144000, 6858000, &tx)
        .expect("compile failed");

    assert_eq!(vector_slide.width_emu, 9144000);
    assert_eq!(vector_slide.height_emu, 6858000);

    // Verify background
    match vector_slide.background {
        Some(VectorBackground::Solid([r, g, b, a])) => {
            assert_eq!([r, g, b, a], [0x11, 0x22, 0x33, 255]);
        }
        _ => panic!("Expected solid background"),
    }

    // Verify 3 commands produced
    assert_eq!(vector_slide.commands.len(), 3);

    // No warning events for supported shapes
    assert!(rx.try_recv().is_err());
}

#[test]
fn test_unsupported_preset_fallback() {
    let (tx, rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    let slide_json = serde_json::json!({
        "index": 0,
        "slideNumber": 1,
        "elements": [
            {
                "type": "shape",
                "x": 0,
                "y": 0,
                "width": 100,
                "height": 100,
                "rotation": 0.0,
                "flipH": false,
                "flipV": false,
                "geometry": "futureFuturisticShape999"
            }
        ],
        "elementSources": [],
        "comments": []
    });

    let slide: Slide = serde_json::from_value(slide_json).unwrap();
    let vs = compiler.compile(&slide, 1000, 1000, &tx).unwrap();
    assert_eq!(vs.commands.len(), 1);

    // A WarningEvent::UnsupportedPreset should be sent
    let warning = rx.try_recv().expect("expected warning event");
    match warning {
        WarningEvent::UnsupportedPreset { name } => {
            assert_eq!(name, "futureFuturisticShape999");
        }
        _ => panic!("wrong warning type"),
    }
}
