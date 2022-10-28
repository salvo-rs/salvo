# Writer

```Writer``` is used to write content to ```Response```:

```rust
#[async_trait]
pub trait Writer {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response);
}
````

Compared to ```Handler```:

```rust
#[async_trait]
pub trait Handler: Send + Sync + 'static {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl);
}
````

Their differences are:
- Different uses, ```Writer``` represents writing specific content to ```Response```, which is implemented by specific content, such as strings, error messages, etc. ```Handler``` is used to process the entire request .
- ```Writer``` is created in ```Handler```, it will consume itself when the ```write``` function is called, it is a one-time call. And ```Handler``` is common to all requests;
- ```Writer``` can be used as the content of ```Result``` returned by ```Handler```;
- The ```FlowCtrl``` parameter does not exist in ```Writer```, and cannot control the execution flow of the entire request.

```Piece``` implements ```Writer```, but with less functionality than ```Writer```:

```rust
pub trait Piece {
    fn render(self, res: &mut Response);
}
````

```Piece```'s rendering function just writes data to ```Response```, this process cannot get information from ```Request``` or ```Depot```.