use lazy_static::lazy_static;
use rustler::{NifResult, ResourceArc};
use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard, PoisonError};
use tantivy::tokenizer::{
    AsciiFoldingFilter, Language, LowerCaser, NgramTokenizer, RawTokenizer, RegexTokenizer,
    RemoveLongFilter, SimpleTokenizer, Stemmer, StopWordFilter, TextAnalyzer, TokenizerManager,
    WhitespaceTokenizer,
};

use crate::modules::resources::TokenizerManagerResource;

// Global tokenizer manager singleton and registry tracking
// The TokenizerManager is shared and locked internally: its clones see the
// tokenizers registered later.
lazy_static! {
    static ref GLOBAL_TOKENIZER_MANAGER: TokenizerManager = TokenizerManager::default();
    static ref TOKENIZER_REGISTRY: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
}

// A panic cannot leave the registry half-updated (a set of names, changed by
// single inserts): it stays usable after one.
fn registry() -> MutexGuard<'static, HashSet<String>> {
    TOKENIZER_REGISTRY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

// Helper function to register a tokenizer and track its name
fn register_tokenizer_with_tracking<T>(name: &str, tokenizer: T)
where
    TextAnalyzer: From<T>,
{
    GLOBAL_TOKENIZER_MANAGER.register(name, tokenizer);
    registry().insert(name.to_string());
}

/// Create a new tokenizer manager
#[rustler::nif]
pub fn tokenizer_manager_new() -> ResourceArc<TokenizerManagerResource> {
    let manager = TokenizerManager::default();
    ResourceArc::new(TokenizerManagerResource { manager })
}

/// Register a simple tokenizer
#[rustler::nif]
pub fn register_simple_tokenizer(name: String) -> NifResult<String> {
    let tokenizer = SimpleTokenizer::default();
    register_tokenizer_with_tracking(&name, tokenizer);
    Ok(format!(
        "Simple tokenizer '{}' registered successfully",
        name
    ))
}

/// Register a whitespace tokenizer
#[rustler::nif]
pub fn register_whitespace_tokenizer(name: String) -> NifResult<String> {
    let tokenizer = WhitespaceTokenizer::default();
    register_tokenizer_with_tracking(&name, tokenizer);
    Ok(format!(
        "Whitespace tokenizer '{}' registered successfully",
        name
    ))
}

/// Register a regex tokenizer
#[rustler::nif]
pub fn register_regex_tokenizer(name: String, pattern: String) -> NifResult<String> {
    match RegexTokenizer::new(&pattern) {
        Ok(tokenizer) => {
            register_tokenizer_with_tracking(&name, tokenizer);
            Ok(format!(
                "Regex tokenizer '{}' registered successfully",
                name
            ))
        }
        Err(e) => Err(rustler::Error::Term(Box::new(format!(
            "Failed to create regex tokenizer: {}",
            e
        )))),
    }
}

/// Register an N-gram tokenizer
#[rustler::nif]
pub fn register_ngram_tokenizer(
    name: String,
    min_gram: usize,
    max_gram: usize,
    prefix_only: bool,
) -> NifResult<String> {
    match NgramTokenizer::new(min_gram, max_gram, prefix_only) {
        Ok(tokenizer) => {
            register_tokenizer_with_tracking(&name, tokenizer);
            Ok(format!(
                "N-gram tokenizer '{}' registered successfully",
                name
            ))
        }
        Err(e) => Err(rustler::Error::Term(Box::new(format!(
            "Failed to create N-gram tokenizer: {}",
            e
        )))),
    }
}

/// Register a tokenizer with filters and configurable long word threshold
#[rustler::nif]
pub fn register_text_analyzer(
    name: String,
    base_tokenizer: String,
    lowercase: bool,
    stop_words_language: Option<String>,
    stemming_language: Option<String>,
    remove_long_threshold: Option<usize>,
) -> NifResult<String> {
    build_and_register_text_analyzer(
        name,
        base_tokenizer,
        lowercase,
        stop_words_language,
        stemming_language,
        remove_long_threshold,
        false,
    )
}

/// `register_text_analyzer`, with ASCII folding if `ascii_folding`.
#[rustler::nif]
pub fn register_text_analyzer_with_folding(
    name: String,
    base_tokenizer: String,
    lowercase: bool,
    stop_words_language: Option<String>,
    stemming_language: Option<String>,
    remove_long_threshold: Option<usize>,
    ascii_folding: bool,
) -> NifResult<String> {
    build_and_register_text_analyzer(
        name,
        base_tokenizer,
        lowercase,
        stop_words_language,
        stemming_language,
        remove_long_threshold,
        ascii_folding,
    )
}

/// The filters apply in this order: lowercasing, stop words, stemming, ASCII
/// folding (after stemming, which relies on the accents), long word removal.
fn build_and_register_text_analyzer(
    name: String,
    base_tokenizer: String,
    lowercase: bool,
    stop_words_language: Option<String>,
    stemming_language: Option<String>,
    remove_long_threshold: Option<usize>,
    ascii_folding: bool,
) -> NifResult<String> {
    let stop_words = match stop_words_language.as_deref() {
        Some(lang) => Some(
            parse_language(lang)
                .and_then(StopWordFilter::new)
                .ok_or_else(|| {
                    rustler::Error::Term(Box::new(format!(
                        "Unsupported stop words language: {}",
                        lang
                    )))
                })?,
        ),
        None => None,
    };

    let stemmer = match stemming_language.as_deref() {
        Some(lang) => Some(Stemmer::new(parse_language(lang).ok_or_else(|| {
            rustler::Error::Term(Box::new(format!("Unsupported stemming language: {}", lang)))
        })?)),
        None => None,
    };

    let mut builder = match base_tokenizer.as_str() {
        "simple" => TextAnalyzer::builder(SimpleTokenizer::default()).dynamic(),
        "whitespace" => TextAnalyzer::builder(WhitespaceTokenizer::default()).dynamic(),
        _ => {
            return Err(rustler::Error::Term(Box::new(format!(
                "Unsupported base tokenizer: {}. Use 'simple' or 'whitespace'",
                base_tokenizer
            ))))
        }
    };

    if lowercase {
        builder = builder.filter_dynamic(LowerCaser);
    }
    if let Some(filter) = stop_words {
        builder = builder.filter_dynamic(filter);
    }
    if let Some(filter) = stemmer {
        builder = builder.filter_dynamic(filter);
    }
    if ascii_folding {
        builder = builder.filter_dynamic(AsciiFoldingFilter);
    }
    if let Some(threshold) = remove_long_threshold {
        builder = builder.filter_dynamic(RemoveLongFilter::limit(threshold));
    }

    register_tokenizer_with_tracking(&name, builder.build());
    Ok(format!("Text analyzer '{}' registered successfully", name))
}

/// The tokenizer manager holding the registered tokenizers, given to the
/// indexes so that their fields can use them. A clone shares the tokenizers:
/// one registered later is seen by the indexes already created.
pub fn global_tokenizer_manager() -> TokenizerManager {
    GLOBAL_TOKENIZER_MANAGER.clone()
}

/// Get list of registered tokenizers
#[rustler::nif]
pub fn list_tokenizers() -> Vec<String> {
    registry().iter().cloned().collect()
}

/// Test tokenization with a registered tokenizer
#[rustler::nif(schedule = "DirtyCpu")]
pub fn tokenize_text(tokenizer_name: String, text: String) -> NifResult<Vec<String>> {
    match GLOBAL_TOKENIZER_MANAGER.get(&tokenizer_name) {
        Some(mut tokenizer) => {
            let mut token_stream = tokenizer.token_stream(&text);
            let mut tokens = Vec::new();

            while let Some(token) = token_stream.next() {
                tokens.push(token.text.clone());
            }

            Ok(tokens)
        }
        None => Err(rustler::Error::Term(Box::new(format!(
            "Tokenizer '{}' not found. Register it first.",
            tokenizer_name
        )))),
    }
}

/// Tokenize text and return detailed token information
#[rustler::nif(schedule = "DirtyCpu")]
pub fn tokenize_text_detailed(
    tokenizer_name: String,
    text: String,
) -> NifResult<Vec<(String, usize, usize)>> {
    match GLOBAL_TOKENIZER_MANAGER.get(&tokenizer_name) {
        Some(mut tokenizer) => {
            let mut token_stream = tokenizer.token_stream(&text);
            let mut tokens = Vec::new();

            while let Some(token) = token_stream.next() {
                tokens.push((token.text.clone(), token.offset_from, token.offset_to));
            }

            Ok(tokens)
        }
        None => Err(rustler::Error::Term(Box::new(format!(
            "Tokenizer '{}' not found. Register it first.",
            tokenizer_name
        )))),
    }
}

/// Register common tokenizers with sensible defaults
#[rustler::nif(schedule = "DirtyCpu")]
pub fn register_default_tokenizers() -> NifResult<String> {
    // Register basic tokenizers. "default" and "raw" are Tantivy's own: the
    // indexes get these tokenizers (see global_tokenizer_manager), so they must
    // not change how the fields using them are indexed.
    register_tokenizer_with_tracking(
        "default",
        TextAnalyzer::builder(SimpleTokenizer::default())
            .filter(RemoveLongFilter::limit(40))
            .filter(LowerCaser)
            .build(),
    );
    register_tokenizer_with_tracking("simple", SimpleTokenizer::default());
    register_tokenizer_with_tracking("keyword", SimpleTokenizer::default()); // Add keyword tokenizer expected by tests
    register_tokenizer_with_tracking("whitespace", WhitespaceTokenizer::default());
    register_tokenizer_with_tracking("raw", RawTokenizer::default());

    // Register language-specific stemming tokenizers
    let languages = ["en", "fr", "de", "es", "it", "pt", "ru"];
    for lang in &languages {
        if let Some(language) = parse_language(lang) {
            let stem_name = format!("{}_stem", lang);
            let tokenizer = TextAnalyzer::builder(SimpleTokenizer::default())
                .filter(LowerCaser)
                .filter(Stemmer::new(language))
                .build();
            register_tokenizer_with_tracking(&stem_name, tokenizer);
        }
    }

    // Register common text analyzers
    if let Some(en_lang) = parse_language("en") {
        let en_analyzer = TextAnalyzer::builder(SimpleTokenizer::default())
            .filter(LowerCaser)
            .filter(StopWordFilter::new(en_lang).unwrap())
            .filter(Stemmer::new(en_lang))
            .build();
        register_tokenizer_with_tracking("en_text", en_analyzer);
    }

    Ok("Default tokenizers registered successfully".to_string())
}

// Helper function to parse language strings
fn parse_language(lang: &str) -> Option<Language> {
    match lang.to_lowercase().as_str() {
        "en" | "english" => Some(Language::English),
        "fr" | "french" => Some(Language::French),
        "de" | "german" => Some(Language::German),
        "es" | "spanish" => Some(Language::Spanish),
        "it" | "italian" => Some(Language::Italian),
        "pt" | "portuguese" => Some(Language::Portuguese),
        "ru" | "russian" => Some(Language::Russian),
        "ar" | "arabic" => Some(Language::Arabic),
        "da" | "danish" => Some(Language::Danish),
        "nl" | "dutch" => Some(Language::Dutch),
        "fi" | "finnish" => Some(Language::Finnish),
        "el" | "greek" => Some(Language::Greek),
        "hu" | "hungarian" => Some(Language::Hungarian),
        "no" | "norwegian" => Some(Language::Norwegian),
        "ro" | "romanian" => Some(Language::Romanian),
        "sv" | "swedish" => Some(Language::Swedish),
        "ta" | "tamil" => Some(Language::Tamil),
        "tr" | "turkish" => Some(Language::Turkish),
        _ => None,
    }
}
