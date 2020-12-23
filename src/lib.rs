#[macro_use]
extern crate lazy_static;

use bloomfilter::Bloom;
use serde::{Deserialize, Serialize};
use wasm_bindgen::__rt::std::collections::HashSet;
use wasm_bindgen::__rt::std::sync::Mutex;
use wasm_bindgen::prelude::*;


// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

const BLOOM_ITEMS_COUNT: usize = 500;
const BLOOM_FP_RATE: f64 = 0.01;

struct Filter {
    id: String,
    bloom: Bloom<String>,
}

lazy_static! {
static ref FILTERS: Mutex<Vec<Filter>> = Mutex::new(Vec::new());
static ref BIT_INDEXES: Mutex<Vec<Vec<u16>>> = Mutex::new(Vec::new());
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[derive(Serialize, Deserialize)]
struct Content {
    id: String,
    text: String,
}

#[wasm_bindgen]
pub fn init_filter() {
    // global系をclearするだけ

    let mut filters = FILTERS.lock().unwrap();
    let mut bit_indexes = BIT_INDEXES.lock().unwrap();
    let bit_len = Bloom::<String>::compute_bitmap_size(BLOOM_ITEMS_COUNT, BLOOM_FP_RATE) * 8;

    filters.clear();
    bit_indexes.clear();

    for _i in 0..bit_len {
        bit_indexes.push(Vec::new());
    }
}

#[wasm_bindgen]
pub fn add_filter(contents: JsValue) {
    let _contents: Vec<Content> = contents.into_serde().unwrap();
    let mut filters = FILTERS.lock().unwrap();
    let mut bit_indexes = BIT_INDEXES.lock().unwrap();
    let bit_len = Bloom::<String>::compute_bitmap_size(BLOOM_ITEMS_COUNT, BLOOM_FP_RATE) * 8;

    // randomの値を全体で統一するために先頭のものを使い回す ない場合は新規に作る (ひょっとしたらもともと同一seedかもしれない)
    let sip_keys: [(u64, u64); 2] = if filters.len() > 0 {
        filters[0].bloom.sip_keys()
    } else {
        let dummy = Bloom::<String>::new_for_fp_rate(BLOOM_ITEMS_COUNT, BLOOM_FP_RATE);

        dummy.sip_keys()
    };

    for content in _contents.iter() {
        let filter = generate_filter(&content.text, sip_keys);
        let bitmap = &filter.bitmap();

        // 取り出したbitmapから転置インデックスを作る
        for i in 0..bit_len {
            if bitmap[i / 8] & (1 << (i % 8)) != 0 {
                bit_indexes.get_mut(i).unwrap().push(filters.len() as u16);
            }
        }

        filters.push(Filter {
            id: content.id.clone(),
            bloom: filter,
        });
    }
}

#[wasm_bindgen]
pub fn search(text: String) -> JsValue {
    let filters = FILTERS.lock().unwrap();

    // 検索時もキーワードから一度フィルタを作る(randomの値を全体で統一するため先頭のフィルタを使い回す)
    let filter = generate_filter(&text, filters[0].bloom.sip_keys());
    let bitmap = filter.bitmap();
    let mut not_found: HashSet<String> = HashSet::new();
    let mut found: HashSet<String> = HashSet::new();
    let bit_indexes = BIT_INDEXES.lock().unwrap();
    let grams = ngrams(&text);
    let grams_len: usize = grams.len();
    let bit_len = Bloom::<String>::compute_bitmap_size(BLOOM_ITEMS_COUNT, BLOOM_FP_RATE) * 8;

    // キーワードから生成したフィルタのbitmapを元に転置インデックスの中を走査する
    for i in 0..bit_len {
        if bitmap[i / 8] & (1 << (i % 8)) != 0 {
            for filter_index in bit_indexes[i].iter() {
                let filter = filters.get(filter_index.clone() as usize).unwrap();

                if found.contains(&filter.id) || not_found.contains(&filter.id) {
                    continue;
                }

                if contains(&filter.bloom, &grams, grams_len) {
                    found.insert(filter.id.clone());
                } else {
                    // 転置インデックスには同じものがたくさん入っているため、"見つからなかった方"も覚えておく
                    not_found.insert(filter.id.clone());
                }
            }
        }
    }

    JsValue::from_serde(&found).unwrap()
}

fn contains(bloom: &Bloom<String>, grams: &HashSet<String>, grams_len: usize) -> bool {
    let count: usize = grams.iter().map(|gram| {
        if bloom.check(&gram) {
            1
        } else {
            0
        }
    }).sum();

    // ngramで分割したすべてのワードが含まれるという扱いの場合trueを返す
    count == grams_len
}

fn generate_filter(text: &String, sip_keys: [(u64, u64); 2]) -> Bloom<String> {
    let words: HashSet<String> = ngrams(text);
    let dummy = Bloom::<String>::new_for_fp_rate(BLOOM_ITEMS_COUNT, BLOOM_FP_RATE);

    // randomの値を統一するために使い回す
    let mut bloom = Bloom::<String>::from_existing(
        dummy.bitmap().as_ref(),
        dummy.number_of_bits(),
        dummy.number_of_hash_functions(),
        sip_keys);

    bloom.clear();

    for word in words.iter() {
        bloom.set(&word);
    }

    bloom
}

const N: usize = 3;

fn ngrams(text: &String) -> HashSet<String> {
    let mut grams: HashSet<String> = HashSet::new();
    let mut word: String = String::new();

    for c in text.chars() {
        match c {
            c if c.is_whitespace() || c.is_control() => {
                // スペースなどで区切られていたらその前後は別の単語として扱う(もーちょい記号を入れられるが今回は、、)
                let count = word.chars().count();
                if count > 0 && count < N {
                    grams.insert(word.clone());
                    word.clear();
                }
            }
            _ => {
                // 全角→半角とかもあるが、実験の実装なのでやらない
                if c.is_uppercase() {
                    word.push_str(c.to_lowercase().to_string().as_str());
                } else {
                    word.push(c);
                }

                let count = word.chars().count();
                if count >= N {
                    if count > N {
                        word.remove(0);
                    }

                    grams.insert(word.clone());
                }
            }
        }
    }

    grams
}
