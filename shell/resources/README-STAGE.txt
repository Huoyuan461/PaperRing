# 本目录是构建时才填充的资源位,仓库里不存大文件:
#   app.jar —— gradle 构建出的后端 bootJar(app/core/build/libs/stirling-pdf-*.jar 改名放进来)
#   jre/    —— jlink 从 Temurin JDK 25 裁剪出的运行时(bin/java.exe 等)
# 本地手动打壳:先把这两样按上面布局放好再跑 tauri build;
# 云端打壳:见 ../.github/workflows/release.yml 的 Stage resources 步骤(自动完成)。
