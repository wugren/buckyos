# websdk 改进 TODO：补齐构造侧能力 + make_config 去 buckycli 化

> 状态：规划完成，未开工（2026-06-11）
> 适用：交给 CodeAgent 逐任务开发。每个任务自包含，按编号顺序做，T 间依赖已标注。

## 0. 背景与总目标

`buckyos/src/make_config.py` 目前依赖两个外部工具：
1. **buckycli**（Rust，需先 `build.py` 编译）—— 7 个子命令，全部实现在
   `buckyos/src/kernel/buckyos-api/src/test_config.rs`（+ `buckycli/src/package_cmd.rs` 的 set_pkg_meta）
2. **CertManager**（Python，buckyos_devkit）—— TLS CA/证书生成

总目标：**websdk 补齐构造侧能力，make_config 用 TS 重写**，消除上述两个 bootstrap 依赖。

设计前提（已和 owner 确认）：
- websdk 的支持要求是**产品级**。构造身份文档的逻辑与真实 web 激活流程同构，
  所以 namelib/provision 按产品级 API 设计，不是 test scaffolding。
- 双实现的格式同步**不需要专门 CI**：make_config 是高频开发脚本，生成文件直接被
  node_daemon 消费，漂移当天暴露。但 Phase 1 内部要有单测。
- Python SDK 不进日程，CertManager 一并迁出。
- **不引入 native 依赖**（sqlite 用 `node:sqlite`，x509 用 `@peculiar/x509`）。

### 涉及仓库（本地 checkout，全部 beta2.2 分支）

| 仓库 | 路径 | 角色 |
|---|---|---|
| buckyos-websdk | `~/project/buckyos-websdk` | **开发目标** |
| buckyos | `~/project/buckyos` | make_config 所在 + buckycli 命令真相源 |
| buckyos-base | `~/project/buckyos-base` | `src/name-lib/` = 身份格式的 Rust 真相源 |
| cyfs-ndn | `~/project/cyfs-ndn` | `src/package-lib/` = pkg meta db 真相源 |

### 关键真相源文件速查

- name-lib：`buckyos-base/src/name-lib/src/{did.rs, zone.rs, device.rs, user.rs, utility.rs, lib.rs}`
- buckycli 命令实现：`buckyos/src/kernel/buckyos-api/src/test_config.rs`
  - sn_db schema: `:53-63`（7 张表）
  - TestKeys 预置密钥: `:149-374`（`get_key_pair_by_id` 在 `:192`）
  - `gen_kernel_service_docs()`: `:410`
  - register_user_to_sn `:936` / register_device_to_sn `:1018`
  - cmd_create_user_env `:1138` / cmd_create_node_configs `:1199` / cmd_create_sn_configs `:1259`
- set_pkg_meta：`buckyos/src/tools/buckycli/src/package_cmd.rs:385`
- meta_index.db schema：`cyfs-ndn/src/package-lib/src/meta_index_db.rs:52-90`
  （pkg_metas / pkg_versions / author_info 三张表），写入逻辑 `add_pkg_meta` `:608`
- websdk 现有可复用件：
  - `src/runtime.ts:1218` 附近 `signJwtWithEd25519`（EdDSA JWT 签名，node:crypto）
  - `src/types.ts` 全部身份文档的 TS 类型 + parse/validate guards
  - `src/ndn_types.ts:800` `buildObjId`（canonicalize/RFC8785 based）
  - `src/namelib.ts` —— **0 字节占位文件，自 2024-12 首 commit 预留，Phase 1 填这里**

### 格式硬约束（所有任务通用）

- 密钥：Ed25519，私钥 **PKCS8 PEM**，公钥 JWK `{"kty":"OKP","crv":"Ed25519","x":"<base64url 无 padding 的 32B>"}`
- JWT：header `{"alg":"EdDSA","typ":"JWT"}`，base64url 无 padding；签名 key 注意是
  **owner 私钥签 device 文档**（不是 device 自己的 key）
- DID scheme：`did:web:` / `did:bns:` / `did:dev:`；`did:bns:` ↔ hostname 转换规则以
  `did.rs` 为准（make_config.py 的 `did_host_to_real_host` 只是其退化版）
- JSON 文件落盘格式与 Rust serde 输出对齐（字段名/类型必须一致；字段顺序无关）

---

## Phase 1 — 填充 `namelib.ts`（name-lib 的 TS 移植）

> 全 Phase 真相源：`buckyos-base/src/name-lib/src/`。目标文件：`websdk/src/namelib.ts`
> （太大可拆 `src/namelib/` 目录，保持 `namelib.ts` 为聚合导出）。
> 浏览器兼容：API 全部 async；keygen/签名优先 `node:crypto`，预留 WebCrypto 路径（Ed25519
> 在现代浏览器 Secure Context 已可用），本阶段 node 跑通即可，但**不要写死 require('node:crypto') 在模块顶层**——沿用 runtime.ts 的 `importNodeModule` 动态导入模式。

- [ ] **T1.1 DID 类**（参考 `did.rs`）
  - scheme 解析/构造、`toHostName()` / `toRawHostName()` / `fromHostNameByBridge(bridge)`
  - 验收：与 Rust 端对同一组输入输出一致（用例覆盖 web/bns/dev 三 scheme + 带 bridge 转换）
- [ ] **T1.2 keygen 与 JWK 工具**（参考 `utility.rs` 的 `generate_ed25519_key_pair` / `get_x_from_jwk`）
  - `generateEd25519KeyPair(): Promise<{privatePem: string, publicJwk: Jwk}>`、`getXFromJwk()`
  - node:crypto `generateKeyPair('ed25519')` + `export({format:'pem',type:'pkcs8'})` + `export({format:'jwk'})`
  - 验收：生成的 PEM 能被 T1.3 签名、被 Rust 端 `ed25519-dalek` 加载（用 buckycli 任一消费路径验证，或单测里用 node:crypto verify 闭环 + 与 TestKeys 已知 PEM 格式逐段对比）
- [ ] **T1.3 EncodedDocument / JWT 编解码**（参考 `lib.rs` 的 `EncodedDocument`、各 config 的 `encode()/decode()`）
  - 复用/重构 runtime.ts 现有 `signJwtWithEd25519`，补 decode（含验签 + 不验签两种）
  - 验收：对 buckycli 生成的真实 JWT（golden fixture，见 T1.7）decode 后字段一致；TS 签的 JWT 能被自身 verify
- [ ] **T1.4 OwnerConfig 构造器**（参考 `user.rs`；TS 类型已有 `types.ts: BuckyOSOwnerConfigDocument`）
  - `newOwnerConfig(username, did, publicJwk, ...)` → 通过 `types.ts` 的 guard
- [ ] **T1.5 ZoneBootConfig / ZoneConfig 构造器**（参考 `zone.rs`）
  - `ZoneBootConfig.encode(ownerPrivatePem)` → JWT；`ZoneConfig.initByBootConfig()`
  - zone_txt_record 构造：`{boot_config_jwt, device_mini_doc_jwt, pkx}`
  - OODDescriptionString 解析（`name@netid` 格式）
- [ ] **T1.6 DeviceConfig / DeviceMiniConfig / NodeIdentityConfig 构造器**（参考 `device.rs`）
  - `DeviceConfig.newByJwk()`、`encode(ownerPrivatePem)`、`DeviceMiniConfig.toJwt()`
  - NodeIdentityConfig 组装（zone_did, owner_public_key, owner_did, device_doc_jwt, device_mini_doc_jwt）
- [ ] **T1.7 golden fixture 单测**（依赖 T1.1-T1.6）
  - 用现版 buckycli 跑一次 `create_user_env` + `create_node_configs`，把输出文件存为
    `websdk/test/fixtures/provision/`；单测断言：TS 构造同参数输出 → 字段级 deep-equal
    （签名值因含随机性/时间戳不比对 raw，比对 decode 后 payload）
  - 验收：`npm test` 绿；fixture 生成步骤写进 fixture 目录 README

## Phase 2 — provision 模块（test_config.rs 的 TS 镜像）

> 目标文件：`websdk/src/provision.ts`（或 `src/provision/`）。仅 node entry 导出（写文件、sqlite）。
> 全 Phase 依赖 Phase 1。

- [ ] **T2.1 TestKeys 预置密钥搬运**（真相源 `test_config.rs:149-374`）
  - 硬编码 dev 密钥对原样搬到 `provision/test_keys.ts`，key id 与 Rust 一致
  - 注：这是 provision 里唯一纯 dev 性质的部分，导出名上做区分（如 `DEV_TEST_KEYS`）
- [ ] **T2.2 `createUserEnv()`**（真相源 `cmd_create_user_env :1138`）
  - 参数对齐 CLI：username/hostname/oodName(支持 `name@netid`)/snBaseHost/rtcpPort/outputDir
  - 产出 5 文件：`user_private_key.pem`、`user_config.json`、`{zone_id}.zone.json`、
    `zone_config.json`、`zone_txt_record.json`
  - 验收：T1.7 的 fixture 对照通过
- [ ] **T2.3 `createNodeConfigs()`**（真相源 `cmd_create_node_configs :1199`，依赖 T2.2 输出）
  - 读 env_dir 的 user_config/zone_config，产出 `{device}/`：`node_private_key.pem`、
    `device_mini_config.jwt`、`node_identity.json`、`node_device_config.json`、`start_config.json`(OOD)
- [ ] **T2.4 sn_db sqlite 封装 + SN 注册**（真相源 schema `:53-63`，register `:936`/`:1018`）
  - `node:sqlite` 实现 `DevSnDb` 类：建 7 张表、`registerUser()`（插 users 表，
    state="active"、activation_code="DIRECT"）、`registerDevice()`（插 devices 表，
    did 从 device_doc_jwt 解出 —— 用 T1.3 decode）
  - 验收：单测建库后 `sqlite3 .schema` 与 Rust 版逐表一致；插入行字段一致
- [ ] **T2.5 `createSnConfigs()`**（真相源 `cmd_create_sn_configs :1259`，依赖 T2.1/T2.4）
  - 产出：`sn_device_config.json`、`sn_private_key.pem`、`params.json`、`sn_db.sqlite3`、
    `.buckycli/user_private_key.pem`、`.buckycli/user_config.json`
- [ ] **T2.6 meta_index.db 封装 + `setPkgMeta()`**
  - 真相源：schema `cyfs-ndn/src/package-lib/src/meta_index_db.rs:52-90`（pkg_metas/
    pkg_versions/author_info），写入语义 `add_pkg_meta :608`；CLI 入口 `package_cmd.rs:385`
  - **注意**：现版是 buckycli 负责建表（make_config.py 只 touch 空文件），TS 版必须自己建表
  - meta object id 计算复用 `ndn_types.ts` 的 `buildObjId`——先写单测确认与 Rust
    `ndn_lib` 算出的 obj id 一致（取 fixture 里的真实 meta json 对比）
  - 验收：对同一 pkg_meta.json，TS 写库后 buckycli/repo-service 能正常读
- [ ] **T2.7 `buildDidDocs()`**（真相源 `gen_kernel_service_docs :410` + 各 `generate_*_service_doc`）
  - 6 个内核服务（verify-hub/scheduler/smb/msg-center/opendan/workflow）的 AppDoc JSON 模板
    照抄，输出 `{did}.doc.json`；DID 由 PackageId unique_name 规则生成（对照
    `package-lib/src/package_id.rs` 的 `unique_name_to_did`）
  - 验收：与 buckycli `build_did_docs` 输出 diff 为空（时间戳类字段除外）

## Phase 3 — x509 证书（替代 Python CertManager）

- [ ] **T3.1 `provision/cert.ts`：`createCa()` / `createCertFromCa()`**
  - 依赖 `@peculiar/x509`（纯 JS/WebCrypto，无 native）
  - 行为对照 buckyos_devkit 的 CertManager（`~/project/buckyos-devkit` 里找 cert_mgr 实现）：
    文件命名 `{name}_ca_cert.pem` / `{name}_ca_key.pem`、支持 hostname + SAN 列表
    （含通配 `*.{zone}` / `*.web3.{base}`）、已存在 CA 则复用
  - 验收：生成的证书 `openssl x509 -text` 检查 SAN/有效期/签发链正确；能被
    cyfs-gateway 的 TLS stack 加载（用 make_config 的 post_gateway.yaml 场景）

## Phase 4 — `make_config.ts` 编排脚本（buckyos 仓库侧）

> 依赖 Phase 1-3 全部完成并发版（或 file: 引用本地 websdk dist）。

- [ ] **T4.1 runtime spike：deno 下 `node:sqlite` 兼容性**
  - 倾向 deno（与 `test/`、buckyos-agent 一致，`test/deno.json` 已有 websdk import 映射）；
    Deno 2.2+ 才支持 `node:sqlite`，先 5 分钟验证；不行则该脚本用 node 跑（仓库已有
    package.json 先例：`test/app_installer_test`）
- [ ] **T4.2 `buckyos/src/make_config.ts` 主体平移**
  - 平移自 `make_config.py`，结构保持：groups 参数表（dev/alice.ood1/bob.ood1/
    charlie.ood1/sn/devtests_ood1/release）、`make_global_env_config`（machine.json/
    active_config.json/meta_index.db.fileobj）、`make_cache_did_docs`、`make_identity_files`、
    `make_repo_cache_file`、`seed_bin_pkg_meta_db`（含从 Cargo.toml 抓 workspace version、
    平台→pkg prefix 映射）、`apply_dev_boot_template_override`（~/.buckycli/buckyos_boot.toml
    的 TOML 多行字符串 merge）、SN 分支（make_sn_configs + 注册 alice/bob/charlie）
  - CLI 参数对齐：`group [--rootfs] [--ca] [--sn_ip]`
  - 全程**不调用 buckycli、不依赖 Python**
- [ ] **T4.3 端到端验收**
  - `make_config.ts dev` 与 `make_config.py dev` 各跑一次，diff 两份 rootfs：
    文件集合一致；JSON 字段级一致（密钥/签名/时间戳除外）
  - 用 TS 版 rootfs 启动 node_daemon（`start.py` 流程）成功拉起 zone —— 这一步就是
    Rust/TS 互通验证的主闭环
- [ ] **T4.4 切换与下线**
  - start.py / CI / VM 文档（`doc/CI/基于VM的开发环境构造.md`）里的 make_config.py 引用切到 TS 版
  - make_config.py 标记 deprecated（保留一个版本周期再删）；buckycli 的 7 个命令**不删**
    （其它消费方可能还在用），只是 make_config 不再依赖

## Phase 5 — SDK 收尾

- [ ] **T5.1 导出面整理**
  - `namelib` 进 universal 导出（index.ts/browser.ts/node.ts）；`provision`（含 cert/sqlite）
    **仅 node.ts** 导出；TestKeys 单独子路径导出避免误入生产 bundle
- [ ] **T5.2 版本与文档**
  - websdk README 增加 provision 章节（一段 quickstart：keygen → createUserEnv →
    createNodeConfigs）；bump minor 版本；buckyos 仓库 `test/deno.json` 等 import 指针更新

---

## 全局验收清单

1. `~/project/buckyos-websdk` `npm test` 全绿（含 golden fixture 对照、obj id 对照、sn_db schema 对照）
2. 全新环境（无 buckycli 二进制、无 buckyos_devkit）只装 deno/node 即可跑通 `make_config.ts dev`
3. TS 生成的 rootfs 能正常启动 node_daemon 并完成 zone boot
4. websdk 无新增 native 依赖；browser bundle 体积不因 provision 增长（tree-shake 验证）

## 已知风险 / 注意事项

- `test_config.rs` 行号基于 2026-06-11 的 beta2.2，开工前先 grep 校准
- DeviceConfig 的 JWT 由 **owner key** 签名，移植时最容易拿错 key —— T1.7 fixture 必须覆盖
- `meta_index.db.fileobj`（make_config 写的"防自动更新"占位 JSON）和 `meta_index.db`
  （sqlite）是两个东西，别混
- bns DID（`alice.bns.did`）与域名 zone（`charlie.me`）两类 zone_id 路径都要测到
- deno 下 `node:crypto` 的 ed25519 PKCS8 导出行为与 node 可能有差异，T4.1 spike 时一并验
