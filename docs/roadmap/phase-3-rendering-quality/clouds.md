# 体积云计划

## 现状

当前云密度到 alpha 的映射过于经验化，缺少显式 transmittance、步长归一化、光照积分和历史有效性处理。

## 目标模型

对每个 ray step 计算：

```text
sigma_t = density * extinction
step_transmittance = exp(-sigma_t * step_length)
transmittance *= step_transmittance
scattering += transmittance * single_scattering * (1 - step_transmittance)
```

最终颜色使用累计 scattering，alpha 使用 `1 - transmittance`；不再直接使用固定 density alpha 缩放替代物理透射。

## 性能路线

1. 先固定世界空间 step length 和最大步数。
2. 半分辨率执行云 raymarch。
3. 使用蓝噪声抖动减少 banding。
4. 用 motion vector 重投影上一帧云颜色。
5. 对历史样本做深度、相位和遮挡有效性判断。
6. 全分辨率 bilateral upsample。
7. 将云阴影作为低频资源提供给地形和水面。

## 失效处理

- 云 Pass 被禁用时输出必须 clear 或 fallback 到天空。
- 相机瞬移、场景切换、天气参数大幅变化时丢弃历史。
- 历史纹理必须 wholesale replacement，不能保留旧 GPU allocation。

## 验证

使用 `scenes/13_clouds.ron` 生成 120 帧截图；比较平均误差、结构相似度、云边缘稳定性和相机移动后的拖影。
