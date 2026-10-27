# Khronos glTF sample models

Copied unmodified from
[KhronosGroup/glTF-Sample-Assets](https://github.com/KhronosGroup/glTF-Sample-Assets)
(upstream `Models/<Name>/glTF-Binary/<Name>.glb`, here `Models/<Name>/<Name>.glb`),
with each model's `LICENSE.md` and `metadata.json`. `Models/` and `LICENSES/`
sit as they do upstream, so each licence file's relative links still resolve. They are fixtures for the scenegraph's
glTF path: small, well known, and each with a property worth testing.

| Model | Size | Licence | Why it is here |
|---|---|---|---|
| `Duck` | 120 KB | SCEA Shared Source 1.0 (`LICENSES/SCEA.txt`), © 2006 Sony | The classic. Its vertices are in centimetres and its own node scales them by 0.01, so it comes out about 1.65 m long and 1.5 m tall. |
| `Fox` | 163 KB | Model CC0-1.0 (PixelMannen); rig and animation CC-BY-4.0 (tomkranis); glTF conversion CC-BY-4.0 (@AsoboStudio, @scurest) | Skinned, with Survey, Walk and Run clips: `pose=Walk@T` plays them. Run lifts its body about 6 cm off its rest height mid-stride, so a running fox needs its feet re-grounded per frame. Its mesh is in centimetres with no scale on its node, so as-is it is 155 units (metres) long — the scale check's case. |
| `CesiumMilkTruck` | 370 KB | CC-BY-4.0, © 2017 Cesium; Cesium logo is a legal mark (`LICENSES/LicenseRef-LegalMark-Cesium.txt`) | Four meshes, four materials (one textured) and its own Z-up to Y-up group. About 4.9 m long. |
| `BoxTextured` | 6 KB | CC-BY-4.0, © 2017 Cesium; Cesium logo is a legal mark (`LICENSES/LicenseRef-LegalMark-Cesium.txt`) | A unit cube with a texture: the smallest model with an image. Its UVs run u = 0..6 under a `REPEAT` sampler, which the importer has to move back into one tile. |

Excluded:
- **The large CC0 models** (Avocado, Lantern, WaterBottle, Corset and others, 8 to 17 MB). Their sizes come from 2048² textures.
- **DamagedHelmet.** Part of it is CC-BY-NC.

Attribution for the CC-BY items is as listed above and in each
`metadata.json`. To use a model in an app folder, copy its `.glb` into that
folder's `models/` as `KEY.glb`, and credit it in a `models/README.md` there.
`apps/scene-play/apps/yard/` holds all four. See the scene-dump skill.
