# Seed-san sample model (VRM)

## Source

- Folder: https://github.com/vrm-c/vrm-specification/tree/master/samples/Seed-san
- File: https://raw.githubusercontent.com/vrm-c/vrm-specification/master/samples/Seed-san/vrm/Seed-san.vrm
- Size: 10,917,800 bytes. SHA-256 `624D0D554BC205BBDC33E22A68A2C3C20EDEBB3E573011EAD8878A65E5329B23` (computed from the file downloaded 2026-10-17).
- Last commit touching the .vrm: `837f156dbce43ad69183ce1bdab549961ae1c1ee`, "Sep 28, 2022" group, "re-added binary files without lfs". Source: https://github.com/vrm-c/vrm-specification/commits/master/samples/Seed-san/vrm/Seed-san.vrm (raw HTML, fetched 2026-10-17). The same commit was also listed by https://api.github.com/repos/vrm-c/vrm-specification/commits?path=samples/Seed-san/vrm/Seed-san.vrm in round 1.

## VRM version

- VRM 1.0. The embedded glTF JSON has `extensions.VRMC_vrm.specVersion = "1.0"`. No legacy `extensions.VRM` (VRM 0.x) block is present.
- Format: glTF 2.0 binary (`glTF` magic, version 2), asset version 2.0.
- Source: the .vrm file itself (JSON chunk parsed with `python -I`).

## License

### Where the license is named

- README (https://raw.githubusercontent.com/vrm-c/vrm-specification/master/samples/Seed-san/README.md), License Information section: "VRM Public License 1.0" linking to https://vrm.dev/en/licenses/1.0/index. Credit line: "Seed-san model by VirtualCast, Inc."
- Embedded `VRMC_vrm.meta` (local file): `name` "Seed-san"; `version` "1"; `authors` ["VirtualCast, Inc."]; `copyrightInformation` "VirtualCast, Inc."; `licenseUrl` "https://vrm.dev/licenses/1.0/"; `allowRedistribution` true; `modification` "allowModificationRedistribution"; `creditNotation` "required"; `commercialUsage` "corporation"; `avatarPermission` "everyone"; `allowPoliticalOrReligiousUsage`, `allowAntisocialOrHateUsage`, `allowExcessivelySexualUsage`, `allowExcessivelyViolentUsage` all true; `thumbnailImage` 14.
- Meta field meanings (VRM meta spec, https://raw.githubusercontent.com/vrm-c/vrm-specification/master/specification/VRMC_vrm-1.0/meta.md, fetched 2026-10-17):
  - `commercialUsage`: "An option that permits use of this model in commercial products." Values: `personalNonProfit` (default; "personal users can use this model only if they are not going to make a profit from its use"), `personalProfit` ("personal users can use this model whether they are going to make a profit from its use or not"), `corporation`. Spec text: "Corporate organization users cannot use this model unless the property is `corporation`."
  - `creditNotation` (default `required`): "An option that requires displaying the credit of this model." `required`: "users of the model must show the credit of the model." `unnecessary`: "users of the model do not have to show the credit of the model."
  - `modification` (default `prohibited`): "An option that gives permission to modify the model, or redistribute the modified model." `allowModificationRedistribution`: "users can modify the model" and "users can redistribute the modified model."
  - `avatarPermission` (default `onlyAuthor`): "The person who can perform as an avatar with this model." `everyone`: "anyone can use this model as an avatar."
  - The spec's License section says: "License settings are settings that are referred from the license document and a licenser of the model can specify." It does not map each meta field to a license section.

### License text (VRM Public License 1.0, https://vrm.dev/en/licenses/1.0/index, "Version 1.0: Established as of August 5, 2021.")

Clause numbering below follows the HTML list structure of that page (fetched 2026-10-17). Quoted text is verbatim from the page.

- Definition 1(h), "License Settings": "means the part of the Licensed Work Data that contains the Licensor’s designation of the subject and terms of the license under this Public License."
- Definition 1(j), "Non-commercial": "means not primarily intended for, and not primarily directed to, commercial gain or monetary compensation."
- Definition 1(k), "Redistribution": "means to provide a VRM file to the public by any means or process that requires permission under the Licensed Rights..."
- Grant, Section 2(a)(1): "to make Model Use of the Licensed Work Data, and to perform any of the following acts that the Licensor specifies in the License Settings". The grant always covers Model Use. Acts 2(a)(1)(A) to (E) depend on the License Settings. Sub-items: 2(a)(1)(A) "Redistribute the Licensed Work Data, in whole or in part;"; 2(a)(1)(B) "make Avatar Use of the Licensed Work Data;"; 2(a)(1)(C) "create or reproduce Adapted Work Data;"; 2(a)(1)(D) "Redistribute any Adapted Work Data; and"; 2(a)(1)(E) "in addition to the foregoing, to conduct any other acts specified by the Licensor in the License Settings."
- Non-commercial condition, Section 2(a)(2): "If the Licensor so specifies in the License Settings, the license in Section 2(a)(1) is granted only if You make the use described in the said Section for Non-commercial purposes."
- Personal-use condition, Section 2(a)(3): "If the Licensor so specifies in the License Settings, the license in Section 2(a)(1) is granted only to You for Your personal use as set forth in the said Section."
- Own terms, Section 2(a)(4): "If the Licensor establishes its own terms in the License Settings other than those set forth in this Public License, the license under Section 2(a)(1) is granted only if You comply with such terms."
- Attribution, Section 3(a)(1): "If the Licensor indicates in the License Settings that this Section applies to You, and if You Redistribute, make Model Use, or make Avatar Use the Licensed Work Data (including in modified form, if applicable), You shall:" Nesting, confirmed in the raw HTML (`<ol type="A">` inside `<ol type="1">`): 3(a)(1)(A) is the lead-in "retain the following if it is supplied by the Licensor with the Licensed Work Data:", and its sub-items (i) to (v) are nested under it. 3(a)(1)(B) and 3(a)(1)(C) are siblings of (A), not sub-items.
  - 3(a)(1)(A)(i) "identification of the creator(s) of the Licensed Work Data and any others designated by the Licensor to receive attribution, in any reasonable manner requested by the Licensor (including by pseudonym, if designated);"
  - 3(a)(1)(A)(ii) "a copyright notice;"
  - 3(a)(1)(A)(iii) "a notice that refers to this Public License;"
  - 3(a)(1)(A)(iv) "a notice that refers to the disclaimer of warranties; and"
  - 3(a)(1)(A)(v) "a URI or hyperlink to the Licensed Work Data to the extent reasonably practicable;"
  - 3(a)(1)(B) "indicate if You modified the Licensed Work Data and retain an indication of any previous modifications; and"
  - 3(a)(1)(C) "indicate the Licensed Work Data is licensed under this Public License, and include the text of, or the URI or hyperlink to, this Public License."
- Section 3(a)(1) also says: "If requested by the Licensor, You must remove any of the information required by Section 3(a)(1)(A) to the extent reasonably practicable."
- Disclaimer of warranties, Section 4(a) (the text 3(a)(1)(A)(iv) refers to): "Unless otherwise separately undertaken by the Licensor, to the extent possible, the Licensor offers the Licensed Work Data as-is and as-available, and makes no representations or warranties of any kind concerning the Licensed Work Data, whether express, implied, statutory, or other." Confirmed in the raw HTML (`<ol type="a">` under `<h2>Section 4.`).

### Conditionality (what the license says about the meta)

- Sections 2(a)(2), 2(a)(3), 2(a)(4) and 3(a)(1) each apply only "If the Licensor so specifies in the License Settings" (or "indicates in the License Settings"). Under Definition 1(h), the License Settings are the part of the VRM file that holds the Licensor's designation, i.e. the embedded meta above.
- The meta does not contain a field named "non-commercial" or "personal use". `commercialUsage` "corporation" is the only meta value that bears on commercial use. Whether it triggers Section 2(a)(2) is a legal question this note does not decide. The license wording (conditional on License Settings) and the meta wording ("corporation" only permits corporate users) do not map one-to-one.
- Mapping `allowRedistribution` to Section 2(a)(1)(A) and `modification` to 2(a)(1)(C)/(D) is an inference from the field names; the license does not state that mapping.
- Items with no meta field: 2(a)(3) personal use. `avatarPermission` "everyone" covers Avatar Use under 2(a)(1)(B), not personal use.

Open question (not decided here): whether the meta values above (`commercialUsage` "corporation", `allowRedistribution` true, `creditNotation` "required") activate the conditions in Sections 2(a)(2), 2(a)(3) and 3(a)(1).

## Expressions (VRM 1.0 preset)

Read from `VRMC_vrm.expressions`. There are 18 preset expressions. The `custom` key is absent.

aa, angry, blink, blinkLeft, blinkRight, ee, happy, ih, lookDown, lookLeft, lookRight, lookUp, neutral, oh, ou, relaxed, sad, surprised

- Morph target binds per preset: `neutral` 0, `blink` 2, all others 1.
- Override flags: `happy` has overrideBlink = "blend"; `relaxed` has overrideBlink = "block" and overrideLookAt = "block"; all other presets have overrideBlink, overrideLookAt and overrideMouth = "none".

## Other extensions (top-level `extensionsUsed` in the glTF JSON)

`VRMC_springBone`, `VRMC_vrm`, `KHR_materials_unlit`, `VRMC_materials_mtoon`, `KHR_texture_transform`, `KHR_materials_emissive_strength`, `VRMC_node_constraint`.

- `VRMC_springBone` (specVersion 1.0): 8 colliders, 2 collider groups, 9 springs, 28 joints in total.
- `VRMC_node_constraint`: a node-level extension, present on 23 nodes (not a top-level extension block).

## Model data (from the embedded glTF JSON)

- 17 materials, 5 meshes, 21 primitives.
- 34,059 vertices (POSITION accessors, counted once per accessor).
- 15 images, all PNG, sizes 256x256 (1), 512x512 (5), 1024x1024 (9); total image bytes 6,952,233 (about 6.95 MB). The .vrm total is 10,917,800 bytes.

## Not found

- Per-file history beyond the last commit that touched the .vrm: not checked.
- The page at https://vrm.dev/licenses/1.0/ (the URL in the file's meta) was not fetched. The license text above comes from https://vrm.dev/en/licenses/1.0/index (the README link).
- Whether `commercialUsage` "corporation" triggers the Section 2(a)(2) condition: NOT FOUND in the license or the VRM meta spec. The spec defines the value but does not connect it to the license text.
- Any statement from VirtualCast, Inc. on commercial use of Seed-san beyond the meta fields: NOT FOUND (searched the README and the license text only).
