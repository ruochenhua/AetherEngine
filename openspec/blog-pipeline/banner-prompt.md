# AetherEngine 博客 Banner 提示词

## 用途
AetherEngine 立项文章 banner / cover 图

## 尺寸要求
- Banner: 1536x768 (2:1 宽屏，博客顶部横幅)
- Cover: 1024x1024 (1:1 方形，文章列表缩略图)

## 英文提示词（直接复制给 Gemini）

### Banner (1536x768)
```
A cinematic wide banner for a tech blog about a modern 3D game engine built with Rust and WebGPU. 

Visual elements:
- Deep dark background (#0a0a1a to #1a1a3e gradient)
- A glowing low-poly triangle in the center emitting soft cyan and purple light
- Abstract wireframe geometric shapes floating around
- Subtle particle dust and light rays
- A faint grid floor suggesting 3D space
- Color palette: deep navy, electric cyan, soft purple, subtle gold accents

Style: clean, minimal, futuristic, technical but elegant. No text, no UI elements, no watermarks. High contrast, suitable for dark mode blog theme.
```

### Cover (1024x1024)
```
A square cover image for a tech blog article about building a 3D rendering engine.

Visual elements:
- Dark background with deep blue and purple tones
- Central focus: a glowing 3D triangle wireframe with vertex colors (red, green, blue gradients)
- Surrounding: abstract floating geometric primitives (cubes, spheres as wireframes)
- Subtle glow effects and light particles
- Color palette: dark navy background, cyan and purple glows, RGB vertex highlights

Style: minimalist tech aesthetic, clean composition, no text, no labels. Suitable as article thumbnail.
```

## 中文提示词（备用）

### Banner
```
一张科技博客横幅图，主题是现代3D渲染引擎。

画面描述：
- 深色背景，深蓝到深紫的渐变
- 中央有一个发光的低多边形三角形，散发青色和紫色的柔和光芒
- 周围漂浮着抽象的几何线框图形
- 微妙的粒子尘埃和光线效果
- 底部有淡淡的网格地面，暗示3D空间
- 配色：深海军蓝、电光青、柔和紫、少量金色点缀

风格：简洁、极简、未来感、技术感但不冰冷。不要文字、不要UI元素、不要水印。高对比度，适合深色主题博客。
```

### Cover
```
一张方形文章封面图，关于从零开始构建3D渲染引擎。

画面描述：
- 深色背景，深蓝紫色调
- 中央是一个发光的3D三角形线框，顶点带有红绿蓝渐变色彩
- 周围漂浮着抽象的几何体线框（立方体、球体）
- 微妙的发光效果和光粒子
- 配色：深色背景、青色和紫色光晕、RGB顶点高亮

风格：极简科技美学，构图干净，不要文字和标签。适合作为文章缩略图。
```

## 生成后处理

生成后需要：
1. 将 Banner 保存为 `aether_banner.png`
2. 放置到博客目录：`E:\Projects\ruochenhua.github.io\blog_source\source\_posts\aether-engine-intro\`
3. 确认 front-matter 中的路径正确：
   ```yaml
   index_img: /2026/05/24/aether-engine-intro/aether_banner.png
   banner_img: /2026/05/24/aether-engine-intro/aether_banner.png
   ```
