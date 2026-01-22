# ============================================
# 多架构构建阶段
# ============================================
FROM --platform=linux/amd64 messense/rust-musl-cross:x86_64-musl AS amd64
WORKDIR /home/rust/src

# 1. 先只复制依赖配置文件，这样依赖变更时才重新编译
COPY Cargo.toml Cargo.lock ./

# 2. 创建虚拟源码来构建依赖（利用Docker缓存）
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo install --path . --root / && \
    rm -rf src

# 3. 复制真正的源码
COPY src ./src

# 4. 重新构建（只编译自己的代码，依赖已缓存）
RUN touch src/main.rs && \
    cargo install --path . --root /

# ============================================
# 运行阶段：使用 scratch 裸镜像
# ============================================
FROM scratch
COPY --from=amd64 /bin/rust-miniflux2feishu /rust-miniflux2feishu

# 暴露端口
EXPOSE 8083

# 启动应用
ENTRYPOINT ["/rust-miniflux2feishu"]
