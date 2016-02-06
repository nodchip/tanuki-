shogi engine(AI player), stronger than Bonanza6 , educational and tiny code(about 2500 lines) , USI compliant engine , capable of being compiled by VC++2015

[やねうら王mini 公式サイト (解説記事等)](http://yaneuraou.yaneu.com/YaneuraOu_Mini/)

[やねうら王公式 ](http://yaneuraou.yaneu.com/)

## VC++2015でコンパイル可能なUSIプロトコル準拠の将棋の思考エンジンです。

- やねうら王nano
    
	- やねうら王nanoは1500行程度で書かれた将棋AIの基本となるプログラムです。αβ以外の枝刈りを一切していません。(R1700程度)

- やねうら王nano plus(作業中2016年2月中旬ごろ完成予定)
	- やねうら王nano plusは探索部300行程度で、かつ、αβ以外の枝刈りを一切しないという条件でどこまで強くなるかの実験です。

- やねうら王mini (作業中2016年2月下旬完成予定)

	- やねうら王miniは、将棋の思考エンジンです。Bonanza6より強く、教育的かつ短いコードで書かれています。2500行程度、探索部400行程度。

- やねうら王classic 

	- やねうら王classicは、ソースコード4000行程度でAperyと同等の棋力を実現するプロジェクトです。(予定)

- やねうら王2016 

	- やねうら王 思考エンジン 2016年版(非公開)

- 連続自動対局フレームワーク

	- 連続自動対局を自動化できます。 

- やねうら王協力詰めsolver
	
	- 『寿限無3』(49909手)も解ける協力詰めsolver →　[解説ページ](http://yaneuraou.yaneu.com/2016/01/02/%E5%8D%94%E5%8A%9B%E8%A9%B0%E3%82%81solver%E3%82%92%E5%85%AC%E9%96%8B%E3%81%97%E3%81%BE%E3%81%99/)

- やねうら王評価関数バイナリ

- CSAのライブラリの[ダウンロードページ](http://www.computer-shogi.org/library/)からダウンロードできます。


##　俺の作業メモ(2016/02/05 15:00現在)

- [ ] やねうら王miniの探索部
- [ ] やねうら王nano plus作りかけ
- [ ] 評価関数の差分計算まだ。

- [x] 2016/02/06・やねうら王nanoで定跡の採択確率を出力するようにした。
- [x] 2016/02/06・やねうら王nanoの定跡を"book.db"から読み込むように変更。
- [x] 2016/02/06・定跡の読み込み等をこのフレームワークの中に入れる
- [x] 2016/02/06・定跡生成コマンド"makebook"
- [x] 2016/02/05・やねうら王nanoの探索部
- [x] 2016/02/05・やねうら王nano秒読みに対応。
- [x] 2016/02/04・やねうら王nanoに定跡追加。
- [x] 2016/02/01・新しい形式に変換した評価関数バイナリを用意する →　用意した → CSAのサイトでライブラリとして公開された。
- [x] 2016/01/31・やねうら王nanoの探索部