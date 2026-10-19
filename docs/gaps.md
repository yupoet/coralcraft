# CoralCraft 差距清单（按优先级排序）

> 供 agent 领活：从上往下取第一项未完成者。每项完成即更新并同步 ROADMAP 数字。
> 排序原则：先纵切（能开文件→能显示→能画），后横向铺功能。

1. **[M1] RIFF 走读器**（Task 9）：合成 fixture 全绿 + 敌意输入错误路径。
2. **[M1] probe 转绿**（Task 10）：五个红灯测试全绿；`docs/reverse/` 首条日志。
3. **[M1] CLI `probe`**（Task 11）：人读 + `--json`，退出码约定。
4. **[M1] cleanroom/assets 门完善**（Task 12）：违规样例驱动的扫描器测试。
5. **[M1] 格式 parity 文档初版**（Task 13）。
6. **[M2] 多页文档模型**：page/spread/layer/object；与 vectorcraft 画板模型的
   差异记录进 `docs/`。
7. **[M2] 原生格式 `.coralcraft`**：JSON、版本化、保存降级；属性测试往返
   （参考 refs/vectorcraft `format/tests/prop_format.rs` 思路，不抄码）。
8. **[M2] 渲染首帧**：软光栅一张静态页面 PNG。
9. **[M3] CDR parse：现代 RIFF 带**——依赖语料（见 brainstorm §8 Q2）。
10. **[M4] egui 壳 + 工具纵切**。
11. **[M5] MCP + CMX + CDR 写评估**。
