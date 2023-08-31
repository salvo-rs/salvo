```shell
# 进入目录
cd examples/db_prisma_orm
# 生成数据库文件和model文件
cargo run --bin prisma-cli -- migrate dev
# 运行server
cargo run --bin db_prisma_orm
# 调用接口
# 添加数据
curl -d '{"username":"test","email": "test@1.com"}' -H 'Content-Type: application/json' -X POST http://0.0.0.0:5800
# 查询数据
curl http://0.0.0.0:5800
```