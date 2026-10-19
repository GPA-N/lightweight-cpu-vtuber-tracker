# Shading settings

The Advanced tab of the settings panel (S) has a Shading group of seven rows. They tune the toon shading of the loaded avatar without editing the model. The code is `crates/avatar/src/settings.rs` (fields, defaults, clamps), `crates/avatar/src/shading.rs` (pure functions that compute the values) and `crates/avatar/src/settings_apply.rs` (writes them onto the materials).

## Master switch

`Shading override` (`shading_override`) is off by default, so every model keeps its own look.

- Off: the model's own MToon values are used. Turning the switch off writes the values captured from the model back onto every material, so the model's own look returns.
- On: the seven settings are written onto every MToon material of the loaded model. Materials that load later get the current settings too.

The other six rows have no effect while the switch is off.

## Rows

Ranges are the clamp ranges. Defaults are the values in a new or missing `settings.json`.

| Row | settings.json key | Range | Default | Written to |
| --- | --- | --- | --- | --- |
| Shading override | `shading_override` | on or off | off | master switch |
| Toon softness | `shade_toony` | 0.0 to 1.0 (shown as 0 to 100 percent) | 0.9 | `shade.toony_factor` |
| Shade shift | `shade_shift` | -1.0 to 1.0 | 0.0 | `shade.shading_shift_factor` |
| Shade strength | `shade_strength` | 0.0 to 1.0 (shown as percent) | 1.0 | shade colour, see below |
| Rim light | `rim_strength` | 0.0 to 2.0 | 1.0 | `rim_lighting.mix_factor` |
| Outline | `outline_width` | 0.0 to 3.0 | 0.0 | outline mode and `outline.width_factor`, see below |
| GI equalisation | `gi_equalization` | 0.0 to 1.0 (shown as percent) | 0.9 | `gi_equalization_factor` |

Step sizes in the panel: 0.05 for Toon softness, Shade strength and GI equalisation; 0.1 for Shade shift, Rim light and Outline.

How the rows map:

- **Shade strength** blends the model's shade colour toward its base colour. At 1.0 the shade colour is the model's own. At 0.0 the shade colour equals the base colour, so the shadow side has no darker tint.
- **Rim light** multiplies the model's own rim mix factor. At 1.0 the model's rim is unchanged, at 0.0 it is off.
- **Outline** 0 turns the outline off. Above 0 the outline is drawn in world coordinates. Its width is the model's own width times the value, where the model's width is first raised to at least 0.002 (so a model with no outline still gets a thin line).

Values in `settings.json` outside a range are clamped to it. A value that is not a number (NaN) falls back to the default. A `settings.json` without these keys loads with the defaults for them.

## Limit: VRM 1.0 models only

These controls change VRM 1.0 models only. The VRM 0.x converter (`crates/avatar/src/vrm0.rs`) does not convert MToon materials yet (OpenSpec task 10.5). A 0.x model therefore has no MToon values to change, and the seven rows do nothing visible on it until task 10.5 is done.
