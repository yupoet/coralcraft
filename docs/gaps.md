# CoralCraft 差距清单（按优先级排序）

> 供 agent 领活：从上往下取第一项未完成者。每项完成即更新并同步 ROADMAP 数字。
> 排序原则：先纵切（能开文件→能显示→能画），后横向铺功能。

- [x] 1. **[M1] RIFF + ZIP 容器走读器**（Task 9 / 9.5）：合成 fixture 全绿 + 敌意输入错误路径 + 10,000 次变异测试。
- [x] 2. **[M1] `coralcraft-cdr::probe` 转绿**（Task 10）：22 个单元测试全绿；`docs/reverse/log-format.md` 首条日志 `REV-0001`。
- [x] 3. **[M1] CLI `probe`**（Task 11）：人读 + `--json`，退出码约定（`0/2/3/4/5`），4 个集成测试全绿。
- [x] 4. **[M1] cleanroom/assets 门完善**（Task 12）：违规样例驱动的扫描器单元测试。
- [x] 5. **[M1] 格式与对标 parity 文档初版**（Task 13）：[`docs/file-format-parity.md`](file-format-parity.md) 与 [`docs/target-app-parity.md`](target-app-parity.md)。
- [ ] 6. **[M2] 多页文档模型（`coralcraft-doc`）**：page/spread/layer/desktop/object + `ObjectId` + `Object::Unsupported` 无损透传节点；与 vectorcraft 画板模型的差异记录进 `docs/`。
- [ ] 7. **[M2] 原生格式 `.coralcraft`**：ZIP（`manifest.json` + `document.json` + `blobs/`）与 JSON、版本化、保存降级；`proptest` 属性测试往返。
- [ ] 8. **[M2] 渲染首帧（`coralcraft-render`）**：`tiny-skia` 软光栅一张静态页面 PNG + `coralcraft-cli render`。
- [ ] 9. **[M3] CDR parse：现代 RIFF 带**——`LIST cmpr` 解压 + `page`/`layr`/`obj ` 解析 + 预烘焙几何兜底。
- [ ] 10. **[M4] egui 壳 + 工具纵切**。
- [ ] 11. **[M5] MCP + CMX + CDR 写评估**。
