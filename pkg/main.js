import {default as init, init_filter, add_filter, search} from './webclientfulltextsearch.js';

// 動画メタデータjsonをimport
import {contents as contents0} from './data/0000.js';
import {contents as contents1} from './data/0001.js';
import {contents as contents2} from './data/0002.js';
import {contents as contents3} from './data/0003.js';
import {contents as contents4} from './data/0004.js';
import {contents as contents5} from './data/0005.js';
import {contents as contents6} from './data/0006.js';
import {contents as contents7} from './data/0007.js';

// 動画idをキーにした動画メタデータのmap
const nii_contents_map = [];

// 検索フィルタ生成用の形式に変換(idとtextにまとめるだけ)
function convert(contents, nii_contents_map, nii_contents) {
    for (const i in nii_contents) {
        const nii_content = nii_contents[i];

        let text = nii_content.title + " ";

        for (const tag in nii_content.tags) {
            text += tag + " ";
        }

        text += nii_content.description;

        const content = {id: nii_content.video_id, text: text};

        contents.push(content);
        nii_contents_map[content.id] = nii_content;
    }
}

async function run() {
    await init('./webclientfulltextsearch_bg.wasm');

    init_filter();

    const contents = [];

    convert(contents, nii_contents_map, contents0);
    convert(contents, nii_contents_map, contents1);
    convert(contents, nii_contents_map, contents2);
    convert(contents, nii_contents_map, contents3);
    convert(contents, nii_contents_map, contents4);
    convert(contents, nii_contents_map, contents5);
    convert(contents, nii_contents_map, contents6);
    convert(contents, nii_contents_map, contents7);

    console.log("add start.");
    const start = performance.now();

    let index = 0;

    // bloom filter生成には時間がかかるため、1つずつ追加している
    // worker的なものにするのが正解なのは分かるが本筋ではないためこれで、、
    const add = function () {
        add_filter([contents[index]]);
        index ++;
        if (index % 1000 == 0) {
            console.log("add " + index + " " + (performance.now() - start) + "ms.");
        }

        if (index >= contents.length) {
            console.log("add end. " + (performance.now() - start) + "ms. " + contents.length + " count.");
        } else {
            setTimeout(add, 0);
        }
    };

    setTimeout(add, 0);
}

// inputからキーワードを取ってきて検索(実験なのでconsoleに出すだけ、、)
function s() {
    const input = document.getElementById("search_word");
    const start = performance.now();
    const ids = search(input.value);

    console.log("search " + input.value);

    input.value = "";

    for (const i in ids) {
        console.log(nii_contents_map[ids[i]]);
    }

    console.log("search end. " + (performance.now() - start) + " ms");
}

// enterを押しても検索するように
function k(e) {
    if (e.keyCode === 13) {
        s();

        return false;
    }

    return true;
}

const input = document.getElementById("search_word");
const button = document.getElementById("search_button");

input.onkeydown = k;
button.onclick = s;


run();
