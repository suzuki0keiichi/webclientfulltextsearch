import { default as init, FullTextSearch } from './webclientfulltextsearch.js';

// 動画メタデータjsonをimport
import { contents as contents0 } from './data/0000.js';
import { contents as contents1 } from './data/0001.js';
import { contents as contents2 } from './data/0002.js';
import { contents as contents3 } from './data/0003.js';
import { contents as contents4 } from './data/0004.js';
import { contents as contents5 } from './data/0005.js';
import { contents as contents6 } from './data/0006.js';
import { contents as contents7 } from './data/0007.js';

const DB_NAME = 'webclientfulltextsearch';
const DB_VERSION = 1;
const STORE_NAME = 'indexes';
const INDEX_KEY = 'main';

// 動画idをキーにした動画メタデータのmap
const niiContentsMap = {};
let searchIndex = null;

// ── IndexedDB helpers ────────────────────────────────────────────────────────

function openDB() {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open(DB_NAME, DB_VERSION);
        request.onupgradeneeded = () => {
            request.result.createObjectStore(STORE_NAME);
        };
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
}

function dbGet(db, key) {
    return new Promise((resolve, reject) => {
        const tx = db.transaction(STORE_NAME, 'readonly');
        const request = tx.objectStore(STORE_NAME).get(key);
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
}

function dbPut(db, key, value) {
    return new Promise((resolve, reject) => {
        const tx = db.transaction(STORE_NAME, 'readwrite');
        const request = tx.objectStore(STORE_NAME).put(value, key);
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
    });
}

// ── Data conversion ──────────────────────────────────────────────────────────

function convertToSearchDocs(niiContentsList) {
    const docs = [];

    for (const niiContents of niiContentsList) {
        for (const item of niiContents) {
            let tags = '';
            if (Array.isArray(item.tags)) {
                tags = item.tags.join(' ');
            } else if (typeof item.tags === 'string') {
                tags = item.tags;
            }

            docs.push({
                id: item.video_id,
                title: item.title || '',
                tags: tags,
                category: item.category || '',
                description: item.description || '',
            });

            niiContentsMap[item.video_id] = item;
        }
    }

    return docs;
}

// ── Status display ───────────────────────────────────────────────────────────

function setStatus(message) {
    const el = document.getElementById('status');
    if (el) el.textContent = message;
}

// ── UI: yield to event loop ──────────────────────────────────────────────────

function yieldToEventLoop() {
    return new Promise(resolve => setTimeout(resolve, 0));
}

// ── Initialization ───────────────────────────────────────────────────────────

async function initialize() {
    await init('./webclientfulltextsearch_bg.wasm');

    setStatus('IndexedDBからインデックスを読み込み中...');

    // IndexedDBからキャッシュされたインデックスを試す
    try {
        const db = await openDB();
        const cached = await dbGet(db, INDEX_KEY);
        if (cached) {
            searchIndex = FullTextSearch.import_index(cached);
            // niiContentsMapの再構築のためデータは読む必要がある
            const allContents = [contents0, contents1, contents2, contents3,
                                 contents4, contents5, contents6, contents7];
            for (const niiContents of allContents) {
                for (const item of niiContents) {
                    niiContentsMap[item.video_id] = item;
                }
            }
            setStatus(`キャッシュから復元完了: ${searchIndex.count}件`);
            return;
        }
    } catch (e) {
        console.warn('IndexedDB読み込み失敗、再構築します:', e);
    }

    // キャッシュがなければ新規構築
    setStatus('インデックス構築中...');
    const start = performance.now();

    searchIndex = new FullTextSearch({
        fields: [
            { name: 'title',       kind: 'text', weight: 2.0 },
            { name: 'tags',        kind: 'text', weight: 1.5 },
            { name: 'category',    kind: 'text', weight: 1.0 },
            { name: 'description', kind: 'text', weight: 1.0 },
        ],
        ngram_size: 3,
    });

    const allContents = [contents0, contents1, contents2, contents3,
                         contents4, contents5, contents6, contents7];
    const docs = convertToSearchDocs(allContents);

    // バッチでインデックスに追加（UIブロック防止）
    const BATCH_SIZE = 500;
    for (let i = 0; i < docs.length; i += BATCH_SIZE) {
        const batch = docs.slice(i, i + BATCH_SIZE);
        searchIndex.add(batch);

        const count = Math.min(i + BATCH_SIZE, docs.length);
        setStatus(`インデックス構築中... ${count}/${docs.length}件`);
        await yieldToEventLoop();
    }

    const elapsed = (performance.now() - start).toFixed(0);
    setStatus(`インデックス構築完了: ${searchIndex.count}件 (${elapsed}ms)`);

    // IndexedDBに保存
    try {
        const db = await openDB();
        const exported = searchIndex.export_index();
        await dbPut(db, INDEX_KEY, exported);
        console.log('インデックスをIndexedDBに保存しました');
    } catch (e) {
        console.warn('IndexedDB保存失敗:', e);
    }
}

// ── Search ───────────────────────────────────────────────────────────────────

function handleSearch() {
    if (!searchIndex) {
        setStatus('インデックスがまだ準備できていません');
        return;
    }

    const input = document.getElementById('search_word');
    const query = input.value.trim();
    if (!query) return;

    const start = performance.now();
    let results;
    try {
        results = searchIndex.search(query, { limit: 100 });
    } catch (e) {
        setStatus(`検索エラー: ${e.message}`);
        return;
    }
    const elapsed = (performance.now() - start).toFixed(1);

    setStatus(`${results.length}件ヒット (${elapsed}ms)`);
    displayResults(results);
}

function displayResults(results) {
    const container = document.getElementById('results');
    container.innerHTML = '';

    if (results.length === 0) {
        container.textContent = '該当なし';
        return;
    }

    for (const result of results) {
        const item = niiContentsMap[result.id];
        const div = document.createElement('div');
        div.className = 'result-item';

        const title = document.createElement('div');
        title.className = 'result-title';
        title.textContent = item ? item.title : result.id;

        const meta = document.createElement('div');
        meta.className = 'result-meta';
        const scoreText = `スコア: ${result.score.toFixed(2)}`;
        const categoryText = item && item.category ? ` | カテゴリ: ${item.category}` : '';
        meta.textContent = scoreText + categoryText;

        const desc = document.createElement('div');
        desc.className = 'result-desc';
        const description = item ? (item.description || '') : '';
        desc.textContent = description.length > 150
            ? description.substring(0, 150) + '...'
            : description;

        div.appendChild(title);
        div.appendChild(meta);
        div.appendChild(desc);
        container.appendChild(div);
    }
}

// ── Event binding ────────────────────────────────────────────────────────────

const input = document.getElementById('search_word');
const button = document.getElementById('search_button');

input.onkeydown = function (e) {
    if (e.key === 'Enter') {
        handleSearch();
        return false;
    }
    return true;
};
button.onclick = handleSearch;

initialize();
