Qiitaでアドベントカレンダー記事を書くために作られたプログラムです。
詳しい情報は[Qiita記事](https://qiita.com/suzuki0keiichi@github/items/f2e8c08ad88ea43e2ce5)を見てください。

# 注意書き
* 動かすことに注力しているのでRustで高コスパな書き方になっていません
* 動かすことに注力しているので変数名も適当になっています(すみません、、)

# 動かし方
1. wasmのビルド(コマンドは後述)
2. pkg/data内にテストデータを配置 
3. httpserv.pyを起動する

## wasmのビルド
```shell
wasm-pack build --release --target web
```
デバッグ情報が欲しいときは--releaseの代わりに--devを指定する

## pkg/data内にテストデータを配置
NIIの動画メタデータのjsonlを一つの配列にしてexportするjsに変換してください。
最終的にidとtextに変えているので、動画のメタデータである必要はないです。
(その代わりmain.jsの加工部分等を書き換える必要はあります)

## httpserv.pyを起動する
これは単純なHTTPサーバーで、wasmがローカルファイルでは動かないために起動しているだけです。
