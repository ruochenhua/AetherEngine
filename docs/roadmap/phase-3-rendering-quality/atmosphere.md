# 大气与天空计划

## 现状

当前大气主要使用逐像素 view-ray/sun-ray 数值积分，并以经验系数补充多次散射；成本随分辨率和采样数增长，天空结果也没有成为 IBL 的统一输入。

## 目标架构

1. Transmittance LUT：二维纹理，输入高度和视线天顶角。
2. Multi-scattering LUT：低频多次散射近似。
3. Sky-view LUT：相机高度和视线方向对应的天空辐射度。
4. Aerial perspective LUT：远景雾和地平线颜色。
5. Sky-to-IBL：从 Sky-view 生成低分辨率 irradiance/specular 环境资源。

## 更新规则

- 太阳方向、地面半径、大气半径、散射系数或相机高度变化时重新计算相关 LUT。
- 静态参数不变时跨帧复用 LUT。
- 场景切换时替换 GPU 纹理并释放旧资源。
- LUT 生成 Pass 必须有明确尺寸、格式和有效版本。

## 迁移步骤

1. 保留旧 raymarch 作为 debug/reference path。
2. 实现 transmittance LUT，并与旧实现做截图比较。
3. 加入 sky-view LUT 和 aerial perspective。
4. 替换硬编码 `assets/hdr/newport_loft.hdr` 的天空环境来源。
5. 让大气天空驱动水面、材质和雾的环境光。

## 完成标准

- LUT 路径与 reference path 的误差有记录。
- 1080p 下大气成本不再随每个像素执行完整双重积分增长。
- 太阳方向变化不会出现旧帧天空或 IBL 残留。
