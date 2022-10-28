# Router

## 什麽是路由

```Router``` 定義了一個 HTTP 請求會被哪些中間件和 ```Handler``` 處理. 這個是 Salvo 裏面最基礎也是最核心的功能.

```Router``` 內部實際上是由一系列過濾器(Filter) 組成, 當所有的過濾器都匹配時, 就認為匹配成功, 如果當前 ```Router``` 有對應的 ```Handler```, 則依次執行 路由上的中間件和 ```Handler```. 如果匹配成功而當前路由又不存在 ```Handler```, 則繼續配置當前 ```Router``` 的子路由, 如此一直下去...

比如:

```rust
Router::with_path("articles").get(list_articles).post(create_article);
```

實際上等同於:

```rust
Router::new()
    // PathFilter 可以過濾請求路徑, 只有請求路徑裏包含 articles 片段時才會匹配成功, 
    // 否則匹配失敗. 比如: /articles/123 是匹配成功的, 而 /articles_list/123 
    // 雖然裏面包含了 articles, 但是因為後面還有 _list, 是匹配不成功的.
    .filter(PathFilter::new("articles"))

    // 在 root 匹配成功的情況下, 如果請求的 method 是 get, 則內部的子路由可以匹配成功, 
    // 並且由 list_articles 處理請求.
    .push(Router::new().filter(filter::get()).handle(list_articles))

    // 在 root 匹配成功的情況下, 如果請求的 method 是 post, 則內部的子路由可以匹配成功, 
    // 並且由 create_article 處理請求.
    .push(Router::new().filter(filter::post()).handle(create_article));
```


## 扁平式定義

我們可以用扁平式的風格定義路由:

```rust
Router::with_path("writers").get(list_writers).post(create_writer);
Router::with_path("writers/<id>").get(show_writer).patch(edit_writer).delete(delete_writer);
Router::with_path("writers/<id>/articles").get(list_writer_articles);
```

## 樹狀式定義

我們也可以把路由定義成樹狀, 這也是推薦的定義方式:

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
這種形式的定義對於復雜項目, 可以讓 Router 的定義變得層次清晰簡單.

在 ```Router``` 中有許多方法調用後會返回自己(Self), 以便於鏈式書寫代碼. 有時候, 你需要根據某些條件決定如何路由, 路由系統也提供了 ```then``` 函數, 也很容易使用:

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
該示例代表僅僅當服務器在 ```admin_mode``` 時, 才會添加創建文章, 編輯刪除文章等路由.

## 從路由中獲取參數

在上面的代碼中, ```<id>``` 定義了一個參數. 我們可以通過 ```Request``` 實例獲取到它的值:

```rust
#[handler]
async fn show_writer(req: &mut Request) {
    let id = req.param::<i64>("id").unwrap();
}
```

```<id>```匹配了路徑中的一個片段, 正常情況下文章的 ```id``` 只是一個數字, 這是我們可以使用正則表達式限制 ```id``` 的匹配規則, ```r"<id:/\d+/>"```. 

對於這種數字類型, 還有一種更簡單的方法是使用  ```<id:num>```, 具體寫法為:
- ```<id:num>```， 匹配任意多個數字字符;
- ```<id:num[10]>```， 只匹配固定特定數量的數字字符，這裏的 10 代表匹配僅僅匹配 10 個數字字符;
- ```<id:num(..10)>```, 代表匹配 1 到 9 個數字字符;
- ```<id:num(3..10)>```, 代表匹配 3 到 9 個數字字符;
- ```<id:num(..=10)>```, 代表匹配 1 到 10 個數字字符;
- ```<id:num(3..=10)>```, 代表匹配 3 到 10 個數字字符;
- ```<id:num(10..)>```, 代表匹配至少 10 個數字字符.

還可以通過 ```<*>``` 或者 ```<**>``` 匹配所有剩余的路徑片段. 為了代碼易讀性性強些, 也可以添加適合的名字, 讓路徑語義更清晰, 比如: ```<**file_path>```. ```<*>``` 與 ```<**>``` 的區別是, 如果路徑是 ```/files/<*rest_path>```, 不會匹配 ```/files```, 而路徑 ```/files/<**rest_path>``` 則可以匹配 ```/files```.

允許組合使用多個表達式匹配同一個路徑片段, 比如 ```/articles/article_<id:num>/```, ```/images/<name>.<ext>```.

## 添加中間件

可以通過路由上的 ```hoop``` 函數添加中間件:

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

在這個例子, 根路由使用 ```check_authed``` 檢查當前用戶是否已經登錄了. 所有子孫路由都會受此中間件影響.

如果用戶只是瀏覽 ```writer``` 的信息和文章, 我們更希望他們無需登錄即可瀏覽. 我們可以把路由定義成這個樣子:

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

盡管有兩個路由都有相同的路徑定義 ```path("articles")```, 他們依然可以被添加到同一個父路由裏.

## 過濾器

```Router``` 內部都是通過過濾器來確定路由是否匹配. 過濾器支持使用 ```or``` 或者 ```and``` 做基本邏輯運算. 一個路由可以包含多個過濾器, 當所有的過濾器都匹配成功時, 路由匹配成功.

網站的路徑信息是一個樹狀機構, 這個樹狀機構並不等同於組織路由的樹狀結構. 網站的一個路徑可能對於多個路由節點. 比如, 在 ```articles/``` 這個路徑下的某些內容需要登錄才可以查看, 而某些有不需要登錄. 我們可以把需要登錄查看的子路徑組織到一個包含登錄驗證的中間件的路由下面. 不需要登錄驗證的組織到另一個沒有登錄驗證的路由下面:


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

路由是使用過濾器過濾請求並且發送給對應的中間件和 ```Handler``` 處理的.

```path``` 和 ```method``` 是兩個最為常用的過濾器. ```path``` 用於匹配路徑信息; ```method``` 用於匹配請求的 Method, 比如: GET, POST, PATCH 等.

我們可以使用 ```and```, ```or ``` 連接路由的過濾器:

```rust
Router::with_filter(filter::path("hello").and(filter::get()));
```

### 路徑過濾器

基於請求路徑的過濾器是使用最頻繁的. 路徑過濾器中可以定義參數, 比如:

```rust
Router::with_path("articles/<id>").get(show_article);
Router::with_path("files/<**rest_path>").get(serve_file)
```

在 ```Handler``` 中, 可以通過 ```Request``` 對象的 ```get_param``` 函數獲取:

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

### Method 過濾器

根據 ```HTTP``` 請求的 ```Method``` 過濾請求, 比如:

```rust
Router::new().get(show_article).patch(update_article).delete(delete_article);
```

這裏的 ```get```, ```patch```, ```delete``` 都是 Method 過濾器. 實際等價於:

```rust
use salvo::routing::filter;

let mut root_router = Router::new();
let show_router = Router::with_filter(filter::get()).handle(show_article);
let update_router = Router::with_filter(filter::patch()).handle(update_article);
let delete_router = Router::with_filter(filter::get()).handle(delete_article);
Router::new().push(show_router).push(update_router).push(delete_router);
```


## 自定義 Wisp

對於某些經常出現的匹配表達式, 我們可以通過 ```PathFilter::register_wisp_regex``` 或者 ```PathFilter::register_wisp_builder``` 命名一個簡短的名稱. 舉例來說, GUID 格式在路徑中經常出現, 正常寫法是每次需要匹配時都這樣:

```rust
Router::with_path("/articles/<id:/[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}/>");
Router::with_path("/users/<id:/[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}/>");
```

每次這麽都要寫這復雜的正則表達式會很容易出錯, 代碼也不美觀, 可以這麽做:

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

僅僅只需要註冊一次, 以後就可以直接通過 ```<id:guid>``` 這樣的簡單寫法匹配 GUID, 簡化代碼的書寫.