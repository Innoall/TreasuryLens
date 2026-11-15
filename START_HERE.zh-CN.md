# 在另一台电脑启动 TreasuryLens

这是完整项目源码包，包含网页、Rust 核心逻辑、两个业务 Lyquid、独立测试部署、Solidity 测试代币、测试与实验记录、启动脚本、固定版本 LDK 源码及网站图文手册。

## 1. 选择运行环境

- **Mac**：Apple Silicon（M 系列）或 Intel。需要 Bash、Python 3.10 或更新版本、curl、tar，以及编译 Rust 所需的系统开发工具。若提示缺少链接器或开发工具，在终端运行 `xcode-select --install`，安装完成后重试。
- **Linux**：arm64 或 x86_64。需要 Python 3.10 或更新版本、Bash、curl、tar、系统 C/C++ 编译器、链接器及 CA 证书。Ubuntu/Debian 可先运行 `sudo apt update`，然后运行 `sudo apt install -y build-essential python3 curl tar ca-certificates pkg-config libssl-dev`。
- **Windows**：本项目脚本使用 Bash，按 **WSL2 + Ubuntu** 方式运行。在管理员 PowerShell 运行 `wsl --install -d Ubuntu`，按系统提示完成安装与重启；进入 Ubuntu 后，按上面的 Linux 步骤准备环境。建议将解压出的 TreasuryLens 复制到 WSL 的 `~/TreasuryLens`，在 Ubuntu 中执行启动命令。浏览器可在 Windows 中访问下方 localhost 地址。没有在原生 Windows 或 WSL 环境实测过，本机验证环境是 Apple Silicon Mac。

首次启动需要联网：下载适配系统架构的 Lyquor v0.8.3、Foundry v1.7.1、Rust 1.99.0 / wasm 编译目标、Solidity 0.8.30、Rust 依赖，以及网络启动所需的 Bartender 发布镜像。无需安装 Docker、钱包插件，也无需付费账户、真实代币或公链资金。Node.js 仅在运行 JavaScript 自动测试时需要，正常启动网站不需要安装前端依赖。

## 2. 解压并启动

**先完整解压 ZIP**，不要在压缩包预览窗口中运行文件。进入解压后的 `TreasuryLens` 文件夹。

Mac 可以双击 `Start-TreasuryLens.command`。如果系统未允许双击执行，使用下面的终端方法即可。

Mac / Linux / WSL 的通用方法是在终端进入项目文件夹，然后运行：

```bash
bash Start-TreasuryLens.sh
```

如果项目路径包含空格，请给路径加引号，例如：

```bash
cd "/你的实际路径/TreasuryLens"
bash Start-TreasuryLens.sh
```

启动器会自动准备工具、启动本地链和四个节点、生成真实测试转账、编译与部署 TreasuryLens 和 Budget Monitor，最后开启网页服务。首次下载和编译耗时取决于网络与电脑性能；看到网页服务地址后再打开浏览器。**保留终端窗口运行**。

通常访问：

**http://127.0.0.1:8080/**

如果终端打印了不同端口，以终端地址为准。建议用 Chrome。网页不能通过双击 `index.html` 获得完整功能，必须保持本地服务运行。另一台电脑需要运行自己的这一套服务；本机的 localhost 地址不会自动指向原电脑。

## 3. 检查启动是否成功

1. 网页应能显示节点状态及默认测试查询。
2. 点击 **Verify outflows**，等待 **Confirmed**。
3. 示例区间应有 **3 条转出事件**，合计 **7,500,000 base units**；6 位小数显示为 **7.5**。
4. 进入 Evidence，运行来源回放核验；Matched 后可查看收款人图、区块图和事件表格。
5. 完整操作说明见 `manual/TreasuryLens_网站全功能图文手册.pdf`；接口及开发说明见 `README.md` 和 `docs/`。

## 4. 关闭、重启和数据

在启动终端按 **Ctrl+C** 关闭项目。重新运行同一启动命令会启动一条新的本地测试链，并重新生成测试数据及部署；旧网络文件会保留在 `.runtime/previous-network.*` 下供检查，网页共享查询历史不自动迁移到新部署。

这个仓库及完整包提供可重建的项目，不包含原电脑的运行数据库、已部署地址、编译缓存或浏览器个人设置。旧实验记录保留在 `evaluation/observed/`，这些是历史验证材料，不代表新电脑已经执行过测试。新电脑启动后，以新生成的 `.runtime/seed.json` 和 `.runtime/deployment.json` 为准。

## 5. 常见问题

- **下载失败 / 编译依赖无法访问**：检查新电脑能否访问 GitHub、Rust 下载站、crates.io 和 ghcr.io；恢复网络后重新执行启动命令。工具安装在项目 `.tools/` 中。
- **提示 Missing python3 / curl / tar**：先安装对应系统基础工具。若提示缺少链接器，安装 Mac 开发工具或 Linux build-essential。
- **提示端口占用**：本地链使用 8545，四节点使用 10080、10087、11080、11087、12080、12087、13080、13087，网页通常使用 8080。关闭旧的本项目启动终端或占用端口的程序后重试；脚本不会自动结束不相关程序。
- **页面连接失败**：确认启动终端仍在运行、部署已结束，并使用终端打印的地址；不要只开启一个静态文件服务器。
- **启动中断 / 部署失败**：先看终端最后的错误，再检查 `.runtime/start-network.log`、`.runtime/deploy.stdout.log`、`.runtime/deploy.stderr.log`。修复环境问题后重新启动。
- **Mac 双击权限提示**：改用终端执行 `bash Start-TreasuryLens.sh`，无需依赖 ZIP 工具保留可执行权限。

## 6. 目录与完整性

- `lyquids/treasurylens/assets/`：HTML、CSS、JavaScript 网页源码。
- `lyquids/treasurylens/`：网站后端与节点核验服务。
- `crates/treasurylens-core/`：确定性计算、日志验证、摘要与证据验证工具。
- `lyquids/budget-monitor/`：独立跨 Lyquid 消费示例；网页预算工具本身在浏览器计算。
- `lyquids/treasurylens-test/`：受控故障测试部署。
- `contracts/`：本地测试代币合约。
- `scripts/`、`tests/`：环境准备、启动、部署、打包及自动测试。
- `vendor/ldk-v0.8.3.tar.gz`：固定 v0.8.3 上游开发库的完整源码，带 LICENSE / NOTICE 和校验文件；启动脚本会校验并解压到 `.tools/ldk/`。完整迁移包还预置了解压后的源码。
- `docs/`、`evaluation/`：架构、安全边界、验收说明和历史实验结果。
- `manual/`：网站全功能图文手册。
- `SOURCE_MANIFEST.json`：打包文件的 SHA-256 清单。

仓库中的 `vendor/` 源码归档及完整包中的 `.tools/ldk/` 避免本地路径依赖缺失；平台专用可执行程序与其余编译依赖仍由启动器下载。请复制整个项目文件夹，包含隐藏文件，保持目录结构不变。
