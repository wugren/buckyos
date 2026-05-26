# BuckyOS 仓库安装包生成实现文档

## 0. 文档定位

本文根据 `notepads/prd_buckyos_packaging.md` 描述 BuckyOS 仓库打包模块在“理想状态”下的实现方案。

本文用于后续生成代码、拆分任务、对比现有代码并制定改造计划。本文不把当前实现的不足作为约束，但实现形态应尽量沿用当前仓库已经存在的脚本入口、目录结构和 `src/bucky_project.yaml` 配置方式，避免为安装包系统引入过重的新抽象。

约定：

- `MUST` 表示必须实现。
- `SHOULD` 表示默认应实现，除非后续明确放弃。
- 本模块只在本地产生未签名安装包。签名、CI 调度、CD 安装验收和 ROM 自动生成不在本文实现范围内。

## 1. 范围与产物

打包模块 MUST 从一个完整的 staging 目录生成平台安装包。staging 目录如何构造不属于本模块职责；打包脚本只读取这个目录。

所有平台都不明确固定版本范围。下表列出本模块需要覆盖的测试平台，其他版本也可能可用。

目标产物：

| 平台 | 测试平台/版本 | 架构 | 格式 | 文件名 |
| --- | --- | --- | --- | --- |
| Windows | Windows 11 25H2 | amd64 | exe | `buckyos-windows-amd64-{version}.exe` |
| macOS | macOS 26 | amd64 | pkg | `buckyos-macos-amd64-{version}.pkg` |
| macOS | macOS 26 | aarch64 | pkg | `buckyos-macos-aarch64-{version}.pkg` |
| Ubuntu 系 | Ubuntu 24.04 | amd64 | deb | `buckyos-linux-amd64-{version}.deb` |
| Ubuntu 系 | Ubuntu 24.04 | aarch64 | deb | `buckyos-linux-aarch64-{version}.deb` |
| Fedora | Fedora 44 | amd64 | rpm | `buckyos-linux-amd64-{version}.rpm` |
| Fedora | Fedora 44 | aarch64 | rpm | `buckyos-linux-aarch64-{version}.rpm` |

`version` MUST 默认来自 staging 内的 `node_daemon --version` 输出。外层脚本在未显式传入版本时执行：

```text
{staging}/buckyos/bin/node-daemon/node_daemon --version
```

Windows 下可执行文件路径为：

```text
{staging}\buckyos\bin\node-daemon\node_daemon.exe
```

如果无法从 `node_daemon --version` 获取版本，且用户没有显式传入版本，打包 MUST 失败。

## 2. 入口脚本

实现 SHOULD 保留当前入口风格：

```text
make_local_pkg.py
src/publish/make_local_win_installer.py
src/publish/make_local_osx_pkg.py
src/publish/make_local_deb.py
src/publish/make_local_rpm.py
```

其中 `src/publish/make_local_rpm.py` 是新增目标脚本。

外层入口：

```bash
python make_local_pkg.py build-pkg [version] [options]
python make_local_pkg.py verify-pkg <package> [options]
python make_local_pkg.py show-target [options]
python make_local_pkg.py show-plan [options]
```

`make_local_pkg.py` MUST 根据当前操作系统自动选择平台脚本：

| 当前系统 | 平台脚本 |
| --- | --- |
| Windows | `src/publish/make_local_win_installer.py` |
| macOS | `src/publish/make_local_osx_pkg.py` |
| Linux deb | `src/publish/make_local_deb.py` |
| Linux rpm | `src/publish/make_local_rpm.py` |

Linux 默认格式为 deb。rpm 通过显式参数选择：

```bash
python make_local_pkg.py build-pkg --format rpm
```

常用参数：

```text
version                 可选；不传时从 node_daemon --version 获取
--arch                  可选；不传时使用本机架构
--build-root            可选；不传时使用默认 staging root
--project               可选；默认 src/bucky_project.yaml
--format                linux 上可选 deb/rpm
--out-dir               可选；默认 publish/
--skip-prepare          可选；表示 build-root 已经是完整 staging
--no-sync-scripts       可选；禁止从配置同步生成脚本
--dry-run               可选；只打印动作
```

本地试用包就是使用缺省参数生成的普通安装包：

- `--arch` 不传，使用本机架构。
- `--build-root` 不传，默认 Linux/macOS 为 `/opt/buckyos`，Windows 为 `C:\opt\buckyos`。
- `--project` 不传，使用 `src/bucky_project.yaml`。
- 生成的安装包必须与 CI 未签名包使用同一套平台脚本和安装逻辑。

## 3. 配置来源

### 3.1 继续使用 `src/bucky_project.yaml`

当前 `src/bucky_project.yaml` 同时被 buckyos-devkit 和打包脚本使用。已知 buckyos-devkit 会读取：

- `name`
- `version`
- `modules`
- `apps`

打包脚本也必须读取 `apps` 中的安装路径规则，尤其是：

- `apps.buckyos.modules`
- `apps.buckyos.data_paths`
- `apps.buckyos.clean_paths`
- `apps.buckycli.*`

这些字段既描述构建/安装项目，也描述安装包的文件覆盖语义。为了避免同一份文件清单在两个配置文件中重复维护，理想实现继续使用 `src/bucky_project.yaml` 作为安装包产品定义来源，不新增完整的 `buckyos_package.yaml`。

本期不新增完整的 `src/publish/buckyos_package.yaml`。未来如果出现纯打包字段且能与 devkit 字段清晰分离，必须另行设计配置边界后再引入新文件。

### 3.2 配置只描述 payload

很多安装包行为固定在脚本中，不进入配置文件：

- 版本来源固定为 `node_daemon --version`。
- homepage 固定在平台脚本中。
- license 由平台脚本读取 `src/publish/{platform}` 下的 license 文件。
- Windows installer engine 固定为 NSIS。
- macOS installer engine 固定为 `pkgbuild` + `productbuild`。
- deb 使用 `dpkg-deb`。
- rpm 在 Ubuntu 24.04 打包机上使用 `rpmbuild` 生成。
- 平台最低版本不通过配置声明。
- 架构通过脚本参数传入，不通过配置声明。
- 不同架构使用不同的 staging root；脚本不在配置中为架构选择 payload。
- `buckyos` service 名固定为 `buckyos`。
- 依赖检测逻辑固定在平台脚本中。
- 静默安装逻辑固定在平台脚本中。
- exit code 固定在本文，平台脚本按本文实现。
- hook 不在配置中列举，由脚本按文件命名规则发现。

### 3.3 `bucky_project.yaml` 约定

打包相关的最小配置形态：

```yaml
name: buckyos
version: "0.6.0"
base_dir: "."

apps:
  buckyos:
    name: buckyos
    rootfs: rootfs/
    default_target_rootfs: "${BUCKYOS_ROOT}"
    modules:
      node_daemon: bin/node-daemon/
      buckycli: bin/buckycli/
      # 其余 modules 按现有 src/bucky_project.yaml 的 modules 列表维护。
    data_paths:
      - etc/node_gateway_info.json
      - data/
      - storage/
      - local/
      - logs/
    clean_paths:
      - data/var/
      - data/cache/
      - local/
      - logs/
      - etc/

  buckycli:
    name: buckycli
    rootfs: rootfs/bin/buckycli/
    default_target_rootfs: "~/buckycli/"
    modules:
      buckycli: buckycli
    data_paths: []
    clean_paths: []

publish:
  macos_pkg:
    apps:
      BuckyOSApp:
        name: BuckyOS Desktop
        type: bundle
        optional: true
        default_selected: true
        src: BuckyOS.app
        default_target: "/Applications/BuckyOS.app"
      buckyos:
        name: Buckyos Service
        type: app
        optional: true
        default_selected: true
        default_target: "/opt/buckyos/"
      buckycli:
        name: buckycli
        type: app
        optional: true
        default_selected: true
        default_target: "/usr/local/bin/"

  win_pkg:
    apps:
      BuckyOSApp:
        name: BuckyOS Desktop
        type: bundle
        optional: true
        default_selected: true
        src: buckyosapp.exe
        default_target: "C:\\Program Files\\BuckyOS\\BuckyOSApp"
      buckyos:
        name: Buckyos Service
        type: app
        optional: true
        default_selected: true
        system_service: true
        default_target: "C:\\Program Files\\BuckyOS\\buckyos"
      buckycli:
        name: buckycli
        type: app
        optional: true
        default_selected: true
        default_target: "C:\\Program Files\\BuckyOS\\buckycli"
```

说明：

- `apps.buckyos.modules` 是 `overwrite` 规则。
- `apps.buckyos.data_paths` 是 `preserve_existing` 规则。
- `apps.buckyos.clean_paths` 是卸载时必须删除的运行缓存、临时数据或可再生状态路径。
- `publish.macos_pkg.apps` 和 `publish.win_pkg.apps` 只描述桌面安装器组件和 payload 入口。
- Linux 不需要 `publish.linux` 配置，固定只打包 `apps.buckyos` 中 service 与 cli 相关内容。
- Windows/macOS 中三个组件均为可选且默认选中；静默安装忽略组件选择，默认安装全部适用组件。
- `cyfs-gateway` 不作为独立用户可见组件；如果 staging 中包含它，必须作为 `buckyos` service payload 的一部分进入安装包。
- 卸载时，`overwrite` 内容和 `clean_paths` MUST 直接删除，不需要询问。
- Windows/macOS 卸载完成上述删除后，MUST 再询问用户是否删除 `data_paths` 内容；默认保留。

### 3.4 配置校验

平台脚本 MUST 校验：

- YAML 可解析。
- `apps.buckyos` 存在。
- `apps.buckyos.modules`、`data_paths`、`clean_paths` 类型合法。
- `publish.win_pkg.apps` 和 `publish.macos_pkg.apps` 中组件字段合法。
- Windows/macOS 必须声明 `BuckyOSApp`、`buckyos`、`buckycli` 三个组件。
- Linux 打包时不得包含 `BuckyOSApp`。
- `type` 只能是 `app` 或 `bundle`。
- `optional` 和 `default_selected` 必须能解析为 bool。
- `default_target` 必须存在。
- `mode` 不在配置中声明，由字段位置决定。
- hook 文件如果存在，必须是普通文件且可被打包脚本读取。

## 4. staging 目录

staging 目录是平台脚本的主输入。

推荐结构：

```text
<staging>/
  BuckyOSApp/
    buckyosapp.exe              # Windows
    BuckyOS.app/                # macOS
  buckyos/
    bin/
    etc/
    data/
    storage/
    local/
    logs/
  buckycli/
    buckycli
```

实际读取规则：

- Windows/macOS bundle 组件：`publish.{platform}.apps.{component}.src` 相对 `<staging>/<component>/` 解析。
- Windows/macOS app 组件：默认读取 `<staging>/<component>/`。
- `buckyos` service 组件：读取 `<staging>/buckyos/`。
- Linux：读取 `<staging>/buckyos/`，并从 `apps.buckyos` 的 `modules`、`data_paths`、`clean_paths` 派生 payload。

staging 中未被 `apps.*` 或 `publish.*.apps.*` 声明的文件 MUST NOT 进入安装包。

组件 source 规则：

| type | 语义 | source 规则 | target 规则 |
| --- | --- | --- | --- |
| `bundle` | 外源组件，不属于 buckyos module | 必须声明 `src`，source root 为 `<staging>/<component>/` | 必须声明 `default_target` |
| `app` | buckyos module 组件 | 不声明 `src`，source 从 `apps.{component}.rootfs` 读取 | 可选 `default_target`；缺省使用 `apps.{component}.default_target_rootfs` |

`bundle` 用于 `BuckyOSApp` 这类已经由其他仓库构建好的外部产物。`app` 用于 `buckyos`、`buckycli` 等由 `bucky_project.yaml` 中 `apps` 节定义的组件。

## 5. 文件安装规则

安装包内的文件只分两类。

| 来源字段 | 安装模式 | 首次安装 | 覆盖安装 |
| --- | --- | --- | --- |
| `apps.buckyos.modules` | `overwrite` | 释放到目标位置 | 覆盖目标位置 |
| `apps.buckyos.data_paths` | `preserve_existing` | 目标不存在时释放 | 目标存在时跳过 |

实现规则：

- `overwrite` 内容直接进入平台 payload 的真实目标路径。
- `preserve_existing` 内容 MUST 进入安装包内的 defaults 区，安装脚本只在真实目标不存在时复制。
- defaults 区推荐名称为 `.buckyos_installer_defaults`。
- `overwrite` 覆盖失败 MUST 导致安装失败。
- `preserve_existing` 目标已存在时跳过，不算失败。
- `preserve_existing` 目标不存在但释放失败 MUST 导致安装失败。
- 未声明的文件或目录不进入安装包。

目录级 `preserve_existing` 的语义：

- 目标目录不存在时，复制整个默认目录。
- 目标目录存在且非空时，跳过整个目录。
- 目标目录存在但为空时，复制默认目录内容。

## 6. 内部 resolved plan

不提供独立的用户配置 manifest，也不提供 `make-manifest` 作为主流程入口。

外层脚本 MUST 在运行时生成一个临时 resolved plan，用于把 `src/bucky_project.yaml`、staging 根目录、当前平台、架构和解析后的组件信息传给平台脚本。这个文件只是内部中间结果：

- 不由用户维护。
- 不作为长期配置文件。
- 构建完成后 SHOULD 删除。
- `show-plan` MUST 打印它，方便调试。

resolved plan SHOULD 至少包含：

```json
{
  "schema_version": 1,
  "project": "src/bucky_project.yaml",
  "staging_root": "<absolute staging root>",
  "platform": "windows|macos|linux",
  "format": "exe|pkg|deb|rpm",
  "arch": "amd64|aarch64",
  "version": "<resolved version>",
  "components": [
    {
      "id": "buckyos",
      "display_name": "Buckyos Service",
      "type": "app",
      "selected_by_default": true,
      "optional": true,
      "source": "<absolute source path>",
      "target": "<target path>",
      "system_service": true
    }
  ],
  "files": [
    {
      "component_id": "buckyos",
      "source": "<absolute source path>",
      "target": "<target path>",
      "mode": "overwrite|preserve_existing"
    }
  ]
}
```

## 7. 打包流水线

`make_local_pkg.py build-pkg` MUST 执行：

1. 解析参数。
2. 检测当前平台和目标平台脚本。
3. 解析架构；不传时使用本机架构。
4. 确定 staging root；不传时使用默认 build-root。
5. 如果未传 `--skip-prepare`，MUST 调用现有本地准备逻辑填充 staging；如果当前平台没有可用准备逻辑，MUST 失败并提示用户传入完整 `--build-root` 或使用 `--skip-prepare`。
6. 从 `node_daemon --version` 获取版本；如果用户显式传入版本则使用传入值。
7. 读取并校验 `src/bucky_project.yaml`。
8. 生成内部 resolved plan。
9. 调用平台脚本生成安装包。
10. 运行平台脚本内置的基础产物校验。
11. 输出安装包路径。

本模块不生成 build report；CI 系统自行收集日志和产物。

## 8. Hook 实现

### 8.1 Hook 文件发现

Hook 不在 YAML 中逐项声明。平台脚本按约定文件名查找，存在则打入安装包，不存在则跳过。

推荐查找目录：

| 平台 | 目录 |
| --- | --- |
| Windows | `src/publish/win_pkg/scripts/` |
| macOS | `src/publish/macos_pkg/scripts/` |
| deb | `src/publish/deb_template/DEBIAN/` 或 `src/publish/linux_deb/scripts/` |
| rpm | `src/publish/rpm_template/` 或 `src/publish/linux_rpm/scripts/` |

Windows/macOS 组件 hook 命名：

```text
{component}_{step}.ps1
{component}_{step}.bat
{component}_{step}.cmd
{component}_{step}.sh
{component}_{step}
```

其中：

- `component` 使用 `publish.{platform}.apps` 的 key，例如 `BuckyOSApp`、`buckyos`、`buckycli`。
- `step` 支持 `preinstall`、`postinstall`。
- macOS 当前允许使用无扩展名脚本，例如 `buckyos_preinstall`。
- Windows 优先使用 PowerShell 脚本。

不支持自定义 `uninstall` hook 文件。卸载逻辑由平台安装器固定实现。

Linux 也支持组件 hook script。打包前，脚本将各组件的 `preinstall` hook 拼接到 deb `preinst` 或 rpm `%pre`，将各组件的 `postinstall` hook 拼接到 deb `postinst` 或 rpm `%post`。拼接顺序使用 resolved plan 中的组件定义顺序，但 hook script 不得依赖该顺序才能正确执行。

### 8.2 Hook 执行规则

- 自定义 hook 只随被安装的组件执行。
- 未选择组件的 hook MUST NOT 执行。
- Hook 执行顺序为组件安装顺序内的 step 顺序。
- Hook 返回非 0 时，安装 MUST 失败。
- 静默安装中 hook MUST 只写日志和返回错误码，不弹窗。
- 图形安装中 hook 失败 MUST 显示错误。
- 覆盖安装前停止 service、停止 BuckyOS Docker 容器等动作属于 `buckyos_preinstall` 或平台脚本内置安装前逻辑。

不单独设计 `preflight` hook。依赖检测与 Retry/Open/Cancel UI 强绑定，必须写在平台安装器脚本中。

### 8.3 BuckyOS service 固定动作

`buckyos` 组件固定为 service 组件。停止旧 service、停止旧进程和停止 BuckyOS Docker 容器等动作 MUST 写入 `buckyos_preinstall` 中，而不是散落在配置文件里。

`buckyos_preinstall` MUST 调用已安装版本的停止脚本。Windows 下固定为：

```text
{installed_buckyos_root}/bin/stop.ps1
```

macOS/Linux 下由平台 hook 提供等价停止动作。停止脚本负责停止现有 `buckyos` service、旧版本 `node_daemon` 进程和 BuckyOS 相关 Docker 容器。执行失败时，preinstall MUST 直接失败并停止安装，不再继续执行后续删除或覆盖动作。

BuckyOS Docker 容器识别规则：

```text
docker ps -aq --filter "label=buckyos.full_appid"
```

卸载前也 MUST 走同一停止逻辑；该逻辑属于平台固定 preuninstall 流程，不通过自定义 `uninstall` hook 文件扩展。

## 9. 依赖检查

依赖检查发生在组件选择之后，只检查已选择组件需要的依赖。静默安装没有组件选择，默认检查全部适用组件依赖。

### 9.1 Windows

Windows 依赖检测固定在 NSIS 脚本中。

Docker CLI 检测：

```cmd
docker --version
```

返回非 0 表示 Docker 缺失。

Docker Engine 检测：

```cmd
docker version
```

Docker CLI 存在但该命令返回非 0，表示 Docker Engine 未启动或当前用户无法访问。

VC++ runtime 检测：

```text
HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64\Installed
HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x64\Installed
```

值为 `1` 表示已安装 Visual C++ 2015-2022 x64 runtime。若未安装且 `src/publish/win_pkg/vcredist_x64.exe` 存在，安装器 MUST 离线执行：

```cmd
vcredist_x64.exe /quiet /norestart
```

必须检查的端口：

```text
3180
80
443
```

安装器应尝试 bind 这些端口；失败时提示端口被占用。

图形安装 UI：

- Docker 缺失：提示用户缺少 Docker Desktop，提供 Open、Retry、Cancel。
- Open 打开 `https://docs.docker.com/desktop/setup/install/windows-install/`。
- Docker Engine 未启动：提示用户 Docker Desktop 未启动或 Docker Engine 不可用，提供 Retry、Cancel。
- VC++ runtime 缺失且不能离线安装：提示用户缺少 Visual C++ 2015-2022 x64 runtime，提供 Open、Retry、Cancel。
- Open 打开 `https://aka.ms/vs/17/release/vc_redist.x64.exe`。
- 端口占用：提示端口号，提供 Retry、Cancel。

静默安装：

- 不弹窗。
- 不打开浏览器。
- 每个失败路径写日志并设置稳定 exit code。

### 9.2 macOS

macOS 依赖检测固定在 Distribution XML 和 `buckyos_preinstall` 中。

检测目标：

- 本机存在 Docker CLI。
- Docker CLI 可以连接到某个 Docker Engine。

检测命令：

```sh
command -v docker
docker info
```

`docker info` 成功即可，不要求 Docker Engine 一定运行在本机，也不要求 `/var/run/docker.sock` 存在。这允许 macOS VM 只安装 Docker CLI，并通过远程 Docker context 或环境变量连接到一台准备好的 Linux Docker Engine。

图形安装 UI：

- Docker CLI 缺失：提示用户安装 OrbStack 或 Docker-compatible CLI，提供 Open、Retry、Cancel。
- Open 打开 `https://orbstack.dev/download`。
- Docker CLI 存在但 `docker info` 失败：提示用户 Docker Engine 未启动、未配置或当前环境无法连接，提供 Retry、Cancel。

静默安装使用 `installer -pkg ... -target /`，失败时由 installer 返回非 0，并在安装日志中记录脚本输出。

### 9.3 Linux

Linux 依赖由包元数据声明，不做额外交互检测。

deb `Depends`：

```text
python3, curl, openssl, psmisc, docker.io | docker-ce | moby-engine
```

rpm `Requires`：

```text
python3
curl
openssl
psmisc
(moby-engine or docker-ce or docker-engine)
```

moby-engine 是 Fedora 仓库中的 Docker/Moby engine 包；docker-ce 是 Docker 官方 Fedora 仓库中的 engine 包；docker-engine 用作兼容旧包名或第三方仓库包名。rpm 平台脚本 MUST 生成 rich dependency 表达式；若 rpmbuild 环境不支持该表达式，构建 MUST 失败并提示修正 rpm 构建环境。

Linux 不额外处理 Docker 已安装但被用户手动停止的情况。

## 10. Exit Code

平台脚本 MUST 尽量使用稳定 exit code。若平台原生安装器无法表达细分错误，至少必须保证成功为 0、失败非 0。

| code | 含义 |
| --- | --- |
| 0 | 成功 |
| 10 | 平台或架构不支持 |
| 11 | 权限不足 |
| 20 | 必需依赖缺失 |
| 21 | 必需依赖已安装但未启动或不可用 |
| 30 | VC++ runtime 缺失或安装失败 |
| 40 | 必需端口被占用 |
| 50 | preinstall/hook 失败 |
| 60 | payload 写入失败 |
| 70 | postinstall/hook 失败 |
| 80 | 卸载失败 |

Windows NSIS 静默安装 MUST 显式 `SetErrorLevel`。macOS 和 Linux 如果无法映射细分 code，脚本至少必须在日志中写明失败原因。

## 11. 安装行为

### 11.1 首次安装

Windows：

- NSIS 图形安装器展示组件选择页。
- `BuckyOSApp`、`buckyos`、`buckycli` 三个组件可任意组合，默认全选。
- 用户可以选择安装路径。
- 如果安装了 `BuckyOSApp`，完成页提供启动选项。
- 如果安装了 `buckyos`，安装器注册并启动 `buckyos` service。

macOS：

- `productbuild` 生成 Distribution pkg。
- 图形安装器展示组件选择页。
- `BuckyOSApp`、`buckyos`、`buckycli` 三个组件可任意组合，默认全选。
- 安装路径固定。
- 安装完成后不自动启动 `BuckyOSApp`；用户从 Launchpad、Finder 或命令行自行启动。
- 如果安装了 `buckyos`，安装器注册并启动 `buckyos` LaunchDaemon。

Linux：

- 用户通过包管理器安装 deb/rpm。
- Linux 固定安装 `buckyos` service 和 `buckycli`。
- Linux 不包含 `BuckyOSApp`。
- post-install 注册、enable 并启动 `buckyos` systemd service。

### 11.2 覆盖安装

覆盖安装支持任意版本覆盖任意版本。

执行顺序：

1. 确定本次安装组件。
2. 检查已选组件依赖。
3. 对 `buckyos` 组件执行安装前停止逻辑。
4. 在实际修改程序文件前完成可提前检测的错误检查。
5. 写入 `overwrite` payload。
6. 从 defaults 区释放缺失的 `preserve_existing` payload。
7. 执行已选组件 postinstall hook。
8. 如果安装了 `buckyos`，启动新版本 service。

失败规则：

- 在实际变更程序文件前失败，必须保持旧版本不变。
- 开始写入程序文件后失败，不要求自动回滚。
- 失败路径不得主动删除用户数据和配置文件。
- 图形安装失败必须提示。
- 静默安装失败必须返回非 0。

### 11.3 卸载

Windows/macOS：

- 支持卸载。
- 停止并移除 `buckyos` service。
- 删除 `overwrite` payload 对应的程序文件。
- 删除 `apps.buckyos.clean_paths` 中声明的路径。
- 删除 `overwrite` payload 和 `clean_paths` 不需要询问。
- 删除完成后，弹框询问用户是否删除 `apps.buckyos.data_paths` 中的内容。
- `data_paths` 默认保留，只有在用户明确选择时才删除。

Linux：

- 卸载结果以包管理器默认行为为准。
- Linux 不提供额外交互卸载能力。

## 12. 静默安装

静默安装通用规则：

- 所有平台支持静默安装。
- 需要管理员/root 权限。
- 不支持选择组件。
- 不支持指定安装路径。
- Windows/macOS 默认安装全部适用组件。
- Linux 默认安装 service 和 cli。
- 成功退出码为 0。
- 失败返回非 0。
- 不弹窗。
- 不打开浏览器。
- 不等待用户输入。
- 写本地日志。

推荐调用：

```powershell
# Windows
Start-Process -FilePath ".\buckyos-windows-amd64-{version}.exe" -ArgumentList "/S" -Wait -PassThru
```

```bash
# macOS
sudo installer -pkg buckyos-macos-{arch}-{version}.pkg -target /

# Ubuntu
sudo apt install ./buckyos-linux-{arch}-{version}.deb

# Fedora
sudo dnf install ./buckyos-linux-{arch}-{version}.rpm
```

## 13. 平台适配器

### 13.1 Windows exe

固定技术：

- NSIS。
- `RequestExecutionLevel admin`。
- 未签名 exe。

必须实现：

- 64-bit Windows 检查。
- 组件选择页。
- 安装路径页。
- license 文件读取：如果 `src/publish/win_pkg/license.txt` 存在则展示，否则跳过。
- 依赖检查和 Open/Retry/Cancel UI。
- 静默安装分支和稳定 exit code。
- `overwrite` 和 `preserve_existing` payload 写入。
- `buckyos` 安装前停止旧 service、scheduled task、启动项和进程。
- `buckyos` postinstall 写入 `BUCKYOS_ROOT`，执行 defaults seed，配置防火墙规则，注册并启动 keepalive scheduled task。
- 卸载器。
- 卸载时默认保留用户数据。

固定 service 机制：

- 本期 Windows 使用 `BuckyOSNodeDaemonKeepAlive` scheduled task 启动 `node_daemon.exe`。
- 同时写入当前用户 Run 注册项作为兼容启动项。
- 需要兼容清理旧版本 Windows service `buckyos`。

`buckycli` 固定安装到当前用户 home 下的 `.buckycli` 目录，并通过写入当前用户 PATH 更新命令行访问路径。

Windows 不需要写普通图形安装日志。静默安装日志 MUST 写到：

```text
%TEMP%\{installer_filename}.log
```

### 13.2 macOS pkg

固定技术：

- `pkgbuild` 构建组件包。
- `productbuild` 构建 Distribution pkg。
- amd64 和 aarch64 分别出包，不生成 universal pkg。
- 未签名 pkg。

必须实现：

- 不需要显式检查 macOS 版本。
- 架构参数驱动产物命名和 payload 选择。
- 组件选择页。
- license 文件读取：如果 `src/publish/macos_pkg/license.html` 存在则展示，否则跳过。
- Docker/OrbStack 检查。
- `buckyos` preinstall 停止旧 LaunchDaemon/LaunchAgent、旧进程和 BuckyOS Docker 容器。
- `buckyos` postinstall 安装 LaunchDaemon `buckyos.service` 并启动。
- `BuckyOSApp` 安装到当前控制台用户的 `~/Applications/BuckyOS.app`，使用户能在自己的 Launchpad 中看到；无控制台用户时回退到 `/Applications/BuckyOS.app`。
- `buckycli` 固定安装到当前用户 home 下的 `.buckycli` 目录，并通过写入当前用户 PATH 更新命令行访问路径。
- 提供卸载脚本。
- 卸载时默认保留用户数据。

固定 service 机制：

- LaunchDaemon label 为 `buckyos.service`。
- plist 路径为 `/Library/LaunchDaemons/buckyos.service.plist`。
- service 启动命令为 `/opt/buckyos/bin/node-daemon/node_daemon --enable_active`。

macOS 不需要写普通图形安装日志。静默安装日志 MUST 写到：

```text
$TMPDIR/{installer_filename}.log
```

macOS 标准 pkg 图形安装器不实现 Windows NSIS 类似的 Open/Retry/Cancel 自定义弹框流程。图形安装检测出错时，安装器 MUST 提示用户具体问题和解决方法，并阻止安装；用户解决问题后重新打开安装器。静默安装检测出错时 MUST 直接返回非 0，并写入静默安装日志。

### 13.3 Linux deb

固定技术：

- `dpkg-deb`。
- 未签名 deb。
- 面向 Ubuntu 系。

必须实现：

- `DEBIAN/control` 写入版本和架构。
- `Depends` 声明 Linux 依赖。
- `preinst` 覆盖安装前停止 `buckyos.service`。
- `postinst` 释放缺失 defaults、写入 systemd unit、enable 并 start service。
- 包内不包含 `BuckyOSApp`。
- 包内不包含未声明文件。

固定 service 机制：

- systemd service 名为 `buckyos.service`。
- unit 路径为 `/etc/systemd/system/buckyos.service`。
- service 启动命令为 `/opt/buckyos/bin/node-daemon/node_daemon --enable_active`。

架构映射：

| canonical | deb |
| --- | --- |
| amd64 | amd64 |
| aarch64 | arm64 |

### 13.4 Linux rpm

固定技术：

- 在 Ubuntu 24.04 打包机上安装 rpm 工具链并使用 `rpmbuild` 生成 rpm。
- 未签名 rpm。
- 面向 Fedora 系，测试平台为 Fedora 44。

必须实现：

- `.spec` 写入版本和架构。
- `Requires` 声明 Linux 依赖。
- `%pre` 覆盖安装前停止 `buckyos.service`。
- `%post` 释放缺失 defaults、写入 systemd unit、enable 并 start service。
- `%preun` / `%postun` 按 rpm 语义处理卸载。
- 包内不包含 `BuckyOSApp`。
- 包内不包含未声明文件。

架构映射：

| canonical | rpm |
| --- | --- |
| amd64 | x86_64 |
| aarch64 | aarch64 |

rpm package metadata 参照 deb 元数据生成：

```spec
Name: buckyos
Version: {rpm_version}
Release: {rpm_release}%{?dist}
Summary: buckyos system software.
License: LicenseRef-BuckyOS
URL: https://github.com/buckyos
BuildArch: {rpm_arch}

Requires: python3
Requires: curl
Requires: openssl
Requires: psmisc
Requires: (moby-engine or docker-ce or docker-engine)

%description
include node_daemon,node_active,cyfs_gateway,app_loader,system_config_service,verify_hub.
with default config files.
```

实现 MUST 将包版本转换为 rpm 可接受的 `Version` / `Release`，但产物文件名仍使用外层解析出的 `{version}`。

rpm 构建环境参考 `doc/CI/ubuntu_2404_rpm_build_env.md`。Ubuntu 24.04 打包机至少安装：

```bash
sudo apt install -y rpm rpm2cpio rpmlint cpio file
```

rpm 安装逻辑参照 deb：同样使用 `overwrite` / `preserve_existing` 规则、同样注册 `buckyos.service`、同样不包含 `BuckyOSApp`。

## 14. 权限、状态和日志

文件权限 MUST 在打包阶段显式规范化，避免依赖构建机状态。

| 类型 | Linux/macOS mode | Windows |
| --- | --- | --- |
| 可执行文件 | `0755` 或沿用现有脚本规范化结果 | 使用安装器默认 ACL |
| 普通文件 | `0644` 或沿用现有脚本规范化结果 | 使用安装器默认 ACL |
| 目录 | `0755` 或沿用现有脚本规范化结果 | 使用安装器默认 ACL |
| 用户数据目录 | 使用现有脚本创建时的默认权限 | 使用安装器默认 ACL |

安装状态：

- Windows 写入 `HKCU\Software\BuckyOS`，至少包含版本和组件安装目录。
- macOS 不写独立 install state；安装状态以 pkg receipt、LaunchDaemon plist 和固定安装路径为准。
- Linux 使用包管理器数据库。
- 安装状态不能替代卸载时的用户数据删除确认。

日志：

- 打包脚本输出 human-readable log。
- 图形安装不要求额外写普通安装日志。
- 静默安装日志统一写到 `$TMP/{installer_filename}.log`，Windows 中 `$TMP` 对应 `%TEMP%`，macOS/Linux 中 `$TMP` 对应 `$TMPDIR` 或 `/tmp`。
- Linux 依赖包管理器日志和 maintainer script 输出。

日志至少记录：

- 产品版本。
- 目标平台/架构。
- 选择组件。
- 依赖检查结果。
- service/container 停止结果。
- hook 执行结果。
- 安装失败原因。
- 最终 exit code。

## 15. 校验

### 15.1 脚本级校验

必须覆盖：

- `src/bucky_project.yaml` 解析和字段校验。
- 平台组件筛选。
- Windows/macOS 三组件存在且默认选中。
- Linux 排除 `BuckyOSApp`。
- `modules` 转换为 `overwrite`。
- `data_paths` 转换为 `preserve_existing`。
- hook 文件发现。
- 依赖检测函数生成。
- 架构映射。
- 产物命名。

### 15.2 产物基础校验

`verify-pkg` MUST 至少检查：

- 安装包存在。
- 安装包格式符合目标平台。
- payload 中不包含未声明路径。
- `data_paths` 不在真实 payload 位置覆盖用户数据，而是进入 defaults 区或等价安装动作。
- Windows/macOS 安装器包含适用组件。
- Linux deb/rpm 不包含 `BuckyOSApp`。
- macOS pkg 包含 Distribution choices。
- deb/rpm 元数据包含依赖声明。

安装验收测试不在本文定义，由 CD 系统文档负责。

## 16. 代码生成任务拆分

建议按以下顺序改造：

1. 统一产物命名和版本解析。
2. 收紧 `src/bucky_project.yaml` 打包字段校验。
3. 将现有临时 manifest 改名或定位为内部 resolved plan。
4. 调整 `make_local_pkg.py` 默认入口和参数。
5. 补齐 Windows/macOS 三组件可选且默认选中。
6. 按本文重写 hook 文件发现和执行规则。
7. 固化依赖检测 UI 与静默 exit code。
8. 补齐 deb 产物校验。
9. 新增 rpm 平台脚本。
10. 补齐平台日志路径和文档。

## 17. 未决项

本文当前不保留 TBD 占位符。后续未明确细节以现有脚本实现、`src/bucky_project.yaml` 和对应平台文档为准。
