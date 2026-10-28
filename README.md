# 纸环 PaperRing

纸环（PaperRing）是在本机运行的 PDF 工具：编辑、合并、拆分、压缩、转换、签名。文件不出本机。

Windows 安装包由 `shell/` 下的桌面壳打包。安装程序上的产品名是 **纸环 PaperRing**，应用标识是 `cn.paperring.app`。

## 上游与许可证

本仓库基于开源 [Stirling-PDF](https://github.com/Stirling-Tools/Stirling-PDF) 的引擎和编辑器，并按其许可证分发。许可证全文见 [LICENSE](LICENSE)，归属说明见 [NOTICE](NOTICE)。

纸环不是 Stirling-PDF，也不是 Stirling Tools 或 Stirling PDF Inc. 的产品。Stirling-PDF 的名称和上游商标仍归其权利人所有。应用里的「关于」页写明了同一句归属。

## 开发

构建、测试和代码风格见 [Developer Guide](docs/developer/DeveloperGuide.md)。常用命令：

- `task dev` — 同时启动后端和前端
- `task check` — lint、类型检查和测试

开发文档里仍会出现上游的包名、日志目录和发行文件名。那些是引擎的标识，不是本产品的名称。
