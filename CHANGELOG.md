### 0.15.0
- core: depot api changed.
- core: change trait ```Handler``` and add ```FlowCtrl```.
- extra: add session supports.

### 0.14.0
- core::http: Request and Response's from_hyper function removed and impl From trait now.
- core::http: Cleanup HttpError.
- core::http: Use FlowState to control write data to response.
- core: remove impl Handler for tuple.
- extra::baisc_auth: 
    - add USERNAME_KEY and BasicAuthDepotExt
    - remove context_key functions.
- extra::jwt_auth:
    - add consts AUTH_CLAIMS_KEY, AUTH_STATE_KEY, AUTH_TOKEN_KEY
    - all extractors add card_methods functions. CookieExtractor ignore PUT, PATH, POST, PATCH methods for csrf.
    - add JwtAuthDepotExt.
    - add enum JwtAuthState.


### 0.13.3

- upgrade to rust edition 2021;
- many apis changed;
- add many unit tests and fix many bugs.

### 0.12.2

- core: add with_path to Router.

### 0.12.0

- core: use multer to parse multipart.
- core: FilePart rename filename to file_name.

### 0.11.6

- extra: fix proxy bug.

### 0.11.5

- extra: pub fs and dir mod in serve.
- fix bug: wrong Stream impl for Body.

### 0.11.4

- feature： Add regex support for wildcard match with * or **.
- use static var for default catchers.

### 0.11.2

- fix bug： in router num with no limit.

### 0.11.1

- extra serve: move default chunk size to name file.
- core fs: rewrite chunked file.
- fix: after handler must be executed.
- remove double check cell.