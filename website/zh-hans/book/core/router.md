# Router

## 什么是路由

```Router``` 定义了一个 HTTP 请求会被哪些中间件和 ```Handler``` 处理. 这个是 Salvo 里面最基础也是最核心的功能.

```Router``` 内部实际上是由一系列过滤器(Filter) 组成, 当所有的过滤器都匹配时, 就认为匹配成功, 如果当前 ```Router``` 有对应的 ```Handler```, 则依次执行 路由上的中间件和 ```Handler```. 如果匹配成功而当前路由又不存在 ```Handler```, 则继续配置当前 ```Router``` 的子路由, 如此一直下去...

比如:

```rust
Router::with_path("articles").get(list_articles).post(create_article);
```

实际上等同于:

```rust
Router::new()
    // PathFilter 可以过滤请求路径, 只有请求路径里包含 articles 片段时才会匹配成功, 
    // 否则匹配失败. 比如: /articles/123 是匹配成功的, 而 /articles_list/123 
    // 虽然里面包含了 articles, 但是因为后面还有 _list, 是匹配不成功的.
    .filter(PathFilter::new("articles"))

    // 在 root 匹配成功的情况下, 如果请求的 method 是 get, 则内部的子路由可以匹配成功, 
    // 并且由 list_articles 处理请求.
    .push(Router::new().filter(filter::get()).handle(list_articles))

    // 在 root 匹配成功的情况下, 如果请求的 method 是 post, 则内部的子路由可以匹配成功, 
    // 并且由 create_article 处理请求.
    .push(Router::new().filter(filter::post()).handle(create_article));
```


## 扁平式定义

我们可以用扁平式的风格定义路由:

```rust
Router::with_path("writers").get(list_writers).post(create_writer);
Router::with_path("writers/<id>").get(show_writer).patch(edit_writer).delete(delete_writer);
Router::with_path("writers/<id>/articles").get(list_writer_articles);
```

## 树状式定义

我们也可以把路由定义成树状, 这也是推荐的定义方式:

```rust
Router::with_path("writers")
    .get(list_writers)
    .post(create_writer)
    .push(
        Router::with_path("<id>")
            .get(show_writer)
            .patch(edit_writer)
            .delete(delete_writer)
            .push(Router::with_path("articles").get(list_writer_articles)),
    );
```
这种形式的定义对于复杂项目, 可以让 Router 的定义变得层次清晰简单.

在 ```Router``` 中有许多方法调用后会返回自己(Self), 以便于链式书写代码. 有时候, 你需要根据某些条件决定如何路由, 路由系统也提供了 ```then``` 函数, 也很容易使用:

```rust
Router::new()
    .push(
        Router::with_path("articles")
            .get(list_articles)
            .push(Router::with_path("<id>").get(show_article)),
    ).then(|router|{
        if admin_mode() {
            router.post(create_article).push(
                Router::with_path("<id>").patch(update_article).delete(delete_writer)
            )
        } else {
            router
        }
    });
```
该示例代表仅仅当服务器在 ```admin_mode``` 时, 才会添加创建文章, 编辑删除文章等路由.

## 从路由中获取参数

在上面的代码中, ```<id>``` 定义了一个参数. 我们可以通过 ```Request``` 实例获取到它的值:

```rust
#[handler]
async fn show_writer(req: &mut Request) {
    let id = req.param::<i64>("id").unwrap();
}
```

```<id>```匹配了路径中的一个片段, 正常情况下文章的 ```id``` 只是一个数字, 这是我们可以使用正则表达式限制 ```id``` 的匹配规则, ```r"<id:/\d+/>"```. 

对于这种数字类型, 还有一种更简单的方法是使用  ```<id:num>```, 具体写法为:
- ```<id:num>```， 匹配任意多个数字字符;
- ```<id:num[10]>```， 只匹配固定特定数量的数字字符，这里的 10 代表匹配仅仅匹配 10 个数字字符;
- ```<id:num(..10)>```, 代表匹配 1 到 9 个数字字符;
- ```<id:num(3..10)>```, 代表匹配 3 到 9 个数字字符;
- ```<id:num(..=10)>```, 代表匹配 1 到 10 个数字字符;
- ```<id:num(3..=10)>```, 代表匹配 3 到 10 个数字字符;
- ```<id:num(10..)>```, 代表匹配至少 10 个数字字符.

还可以通过 ```<*>``` 或者 ```<**>``` 匹配所有剩余的路径片段. 为了代码易读性性强些, 也可以添加适合的名字, 让路径语义更清晰, 比如: ```<**file_path>```. ```<*>``` 与 ```<**>``` 的区别是, 如果路径是 ```/files/<*rest_path>```, 不会匹配 ```/files```, 而路径 ```/files/<**rest_path>``` 则可以匹配 ```/files```.

允许组合使用多个表达式匹配同一个路径片段, 比如 ```/articles/article_<id:num>/```, ```/images/<name>.<ext>```.

## 添加中间件

可以通过路由上的 ```hoop``` 函数添加中间件:

```rust
Router::new()
    .hoop(check_authed)
    .path("writers")
    .get(list_writers)
    .post(create_writer)
    .push(
        Router::with_path("<id>")
            .get(show_writer)
            .patch(edit_writer)
            .delete(delete_writer)
            .push(Router::with_path("articles").get(list_writer_articles)),
    );
```

在这个例子, 根路由使用 ```check_authed``` 检查当前用户是否已经登录了. 所有子孙路由都会受此中间件影响.

如果用户只是浏览 ```writer``` 的信息和文章, 我们更希望他们无需登录即可浏览. 我们可以把路由定义成这个样子:

```rust
Router::new()
    .push(
        Router::new()
            .hoop(check_authed)
            .path("writers")
            .post(create_writer)
            .push(Router::with_path("<id>").patch(edit_writer).delete(delete_writer)),
    )
    .push(
        Router::with_path("writers").get(list_writers).push(
            Router::with_path("<id>")
                .get(show_writer)
                .push(Router::with_path("articles").get(list_writer_articles)),
        ),
    );
```

尽管有两个路由都有相同的路径定义 ```path("articles")```, 他们依然可以被添加到同一个父路由里.

## 过滤器

```Router``` 内部都是通过过滤器来确定路由是否匹配. 过滤器支持使用 ```or``` 或者 ```and``` 做基本逻辑运算. 一个路由可以包含多个过滤器, 当所有的过滤器都匹配成功时, 路由匹配成功.

网站的路径信息是一个树状机构, 这个树状机构并不等同于组织路由的树状结构. 网站的一个路径可能对于多个路由节点. 比如, 在 ```articles/``` 这个路径下的某些内容需要登录才可以查看, 而某些有不需要登录. 我们可以把需要登录查看的子路径组织到一个包含登录验证的中间件的路由下面. 不需要登录验证的组织到另一个没有登录验证的路由下面:


```rust
Router::new()
    .push(
        Router::with_path("articles")
            .get(list_articles)
            .push(Router::new().path("<id>").get(show_article)),
    )
    .push(
        Router::with_path("articles")
            .hoop(auth_check)
            .post(list_articles)
            .push(Router::new().path("<id>").patch(edit_article).delete(delete_article)),
    );
```

路由是使用过滤器过滤请求并且发送给对应的中间件和 ```Handler``` 处理的.

```path``` 和 ```method``` 是两个最为常用的过滤器. ```path``` 用于匹配路径信息; ```method``` 用于匹配请求的 Method, 比如: GET, POST, PATCH 等.

我们可以使用 ```and```, ```or ``` 连接路由的过滤器:

```rust
Router::with_filter(filter::path("hello").and(filter::get()));
```

### 路径过滤器

基于请求路径的过滤器是使用最频繁的. 路径过滤器中可以定义参数, 比如:

```rust
Router::with_path("articles/<id>").get(show_article);
Router::with_path("files/<**rest_path>").get(serve_file)
```

在 ```Handler``` 中, 可以通过 ```Request``` 对象的 ```get_param``` 函数获取:

```rust
#[handler]
pub async fn show_article(req: &mut Request) {
    let article_id = req.param::<i64>("id");
}

#[handler]
pub async fn serve_file(req: &mut Request) {
    let rest_path = req.param::<i64>("**rest_path");
}
```

### Method 过滤器

根据 ```HTTP``` 请求的 ```Method``` 过滤请求, 比如:

```rust
Router::new().get(show_article).patch(update_article).delete(delete_article);
```

这里的 ```get```, ```patch```, ```delete``` 都是 Method 过滤器. 实际等价于:

```rust
use salvo::routing::filter;

let mut root_router = Router::new();
let show_router = Router::with_filter(filter::get()).handle(show_article);
let update_router = Router::with_filter(filter::patch()).handle(update_article);
let delete_router = Router::with_filter(filter::get()).handle(delete_article);
Router::new().push(show_router).push(update_router).push(delete_router);
```


## 自定义 Wisp

对于某些经常出现的匹配表达式, 我们可以通过 ```PathFilter::register_wisp_regex``` 或者 ```PathFilter::register_wisp_builder``` 命名一个简短的名称. 举例来说, GUID 格式在路径中经常出现, 正常写法是每次需要匹配时都这样:

```rust
Router::with_path("/articles/<id:/[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}/>");
Router::with_path("/users/<id:/[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}/>");
```

每次这么都要写这复杂的正则表达式会很容易出错, 代码也不美观, 可以这么做:

```rust
use salvo::routing::filter::PathFilter;

#[tokio::main]
async fn main() {
    let guid = regex::Regex::new("[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}").unwrap();
    PathFilter::register_wisp_regex("guid", guid);
    Router::new()
        .push(Router::with_path("/articles/<id:guid>").get(show_article))
        .push(Router::with_path("/users/<id:guid>").get(show_user));
}
```

仅仅只需要注册一次, 以后就可以直接通过 ```<id:guid>``` 这样的简单写法匹配 GUID, 简化代码的书写.