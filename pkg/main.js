import {default as init, sandbox, generate, search} from './webclientfulltextsearch.js';

const contents = [
    {id: "sm9", text: "そうです私が変なおじさんです"},
    {id: "sm10", text: "私は変なおじさんじゃないですいいかげんにしてください"},
    {id: "sm11", text: "そうです"},
];

async function run() {
    await init('./webclientfulltextsearch_bg.wasm');

    generate(contents);
    sandbox();
}

function s() {
    const input = document.getElementById("search_word");
    const ids = search(input.value);

    input.value = "";

    console.log(ids);
}

const button = document.getElementById("search_button");

button.onclick = s;
run();
