# T0 视觉候选报告（2026-10-11）

- 采集批次：`t0-candidate-final-20261011-62057`（25/25 场景正常退出，候选均为 `reference-pending`）
- 来源提交：`7c09b20e8d7b4236e99680c93bbcf2920548295e`
- runner：`visual-case-runner/2`；系统：macOS x86_64；图形后端：Metal。系统未提供 driver version，因此按实际信息标注为 “Metal backend (driver version unavailable)”。
- 审核原则：本报告只记录候选采集与 provenance，不代表人工视觉通过，也未将候选提升为 reference。
- 本机 HTML 汇总：`tests/reports/t0-candidate-final-20261011-62057.html`；原始 PNG 与逐场景 JSON 位于 `tests/reports/t0-candidate-final-20261011-62057/`。`tests/reports/` 被 Git 忽略，原始图像保留在采集机器；此跟踪报告保留可核验的哈希与采集来源。

| 场景 | 状态 | 尺寸 | Adapter | Candidate SHA-256 |
| --- | --- | ---: | --- | --- |
| `02_multi_object` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `36cfc1464f79de0acab3d02efb9846160034d03873c97fb0705d87807b56ccde` |
| `03_shadow_debug` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `14bda40fad7d8184f1d474a43401cda01aad1b7a682272146d9123f302e6bb1c` |
| `03_shadow_demo` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `0b969e2282e329b0a5b0924e5516aa36bcf1e273d4067f8c9a42689b09f4776e` |
| `04_ibl_debug` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `a5a15b54895613875ab1e62018839c0eb825968deca8f6bb43fb7232329a7d93` |
| `05_ssao_debug_mode14` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `928a1b8428219276118ab37f1b5a75dbda199ee5dacc017512d41bf95256e8f2` |
| `06_ssao_extreme_mode14` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `b6c8982fff8fc2fa3c7190de7857063ef72dabda931906c2b8791a6d39a85561` |
| `07_ssr_debug` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `1d33d40183cd2e4b86941d5d5223319a303588f1bc8520cdb2ba4682a91bbcd5` |
| `08_terrain` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `616f975b63fd5125a28b1763221381750561b75ac14e8b2fddaf82288f321ba3` |
| `09_terrain` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `ad747ba589e9c3f14a4333f85a9b504dfa0786910ce8d1441826477fe1a21bfc` |
| `10_csm` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `2fc45ff218354716984da4fb89cf90bca08655c42a262f437c15a6bfe26cb660` |
| `11_atmosphere` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `9394933723017df4a498bfb6a533096e1ce84baf2043782fce44f30cee29219d` |
| `12_water` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `4e5be036fbe222a4e4083006cec18010cc213cebff2794a93160a032f4739c04` |
| `13_clouds` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `109956f0282656a8388ebc3ecb8a9d5260da1de864b62b8e55970132b4c96440` |
| `14_god_rays` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `fe3f0f6e457820147ab5c32a21edeec2de42560dc6b9c229a4dfc7e8362d7087` |
| `16_terrain_perlin` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `371f80e9da685f7faece7611092cb46d13a381a2dbad139d6a4e0b168a3e8f0c` |
| `17_textured_terrain` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `8090dc12330de1a0166d21f1fadf649923e29815a85e5abdf66f0619e4cd3e95` |
| `18_textured_water` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `d3439213aab88ce086589a4b43fa75a08f9f49eb39593c61d11c66b48ff7d215` |
| `19_water_reflection_test` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `c0f2341dfa98fa8b824b35f58cd2bcfc276566e88e3e5e776f5e2877f65408ac` |
| `20_model_loading_nanosuit` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `9d66e246a07de6e2690f22c04162944a2f3a9e18b79a67f6629f7f506c4e93e4` |
| `20_terrain_water_clouds` | reference-pending | 2560×1440 | Intel(R) Iris(TM) Plus Graphics 650 | `d4deb574ab4a82a3b2cc78c9933d7ea822eabb61936afca6ef6cb2a31b9f0740` |
| `21_ssr_debug_clear_off` | reference-pending | 1280×720 | Intel(R) Iris(TM) Plus Graphics 650 | `1916f40f4fa1c986aacef158d5ac9134aff669fb07f97c17c903ab92dc7b6555` |
| `21_ssr_debug_clear_on` | reference-pending | 1280×720 | Intel(R) Iris(TM) Plus Graphics 650 | `771cb83d79d9bc4b2ac5225a0a163f0b3d2734d57681b453fe2183122d4e318e` |
| `t2_three_local_lights` | reference-pending | 1280×720 | Intel(R) Iris(TM) Plus Graphics 650 | `e3a558bb948c04460138be9baeaa89f374575a6ca3fae5c11d5cc0da6e83e553` |
| `t3_pbr_material_grid` | reference-pending | 1280×720 | Intel(R) Iris(TM) Plus Graphics 650 | `a4e778c4dcc1203f4647472f1041c22dee6196ffd18d5057ab3190e16ba78878` |
| `t7_skinned_shadow_reflection` | reference-pending | 960×540 | Intel(R) Iris(TM) Plus Graphics 650 | `f7415971651848ac7720989d75547361fd027c17a51690b13fd32b5291acba03` |
