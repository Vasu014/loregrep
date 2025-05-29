# Query Engine & Search System

## Overview
Advanced search and query engine providing fast, flexible pattern matching across code structures with support for complex filters, fuzzy matching, and semantic queries.

## Core Architecture

### Query Engine Components
```rust
pub struct QueryEngine {
    search_backends: HashMap<SearchBackend, Box<dyn SearchProvider>>,
    query_parser: QueryParser,
    result_ranker: ResultRanker,
    filter_engine: FilterEngine,
    cache: QueryCache,
    database: Arc<DatabaseManager>,
    config: QueryConfig,
}

pub struct QueryConfig {
    pub default_max_results: usize,
    pub fuzzy_threshold: f64,
    pub enable_caching: bool,
    pub cache_ttl_seconds: u64,
    pub parallel_search: bool,
    pub search_timeout_ms: u64,
}

pub enum SearchBackend {
    Exact,        // Direct string/regex matching
    Fuzzy,        // Levenshtein distance-based
    FullText,     // SQLite FTS
    Semantic,     // Future: vector-based similarity
}
```

## Query Parsing & Language

### Query Language
```rust
pub struct QueryParser {
    grammar: QueryGrammar,
    tokenizer: QueryTokenizer,
}

pub enum QueryExpression {
    // Basic patterns
    Exact(String),
    Regex(regex::Regex),
    Wildcard(String),
    Fuzzy { pattern: String, threshold: f64 },
    
    // Logical operators
    And(Box<QueryExpression>, Box<QueryExpression>),
    Or(Box<QueryExpression>, Box<QueryExpression>),
    Not(Box<QueryExpression>),
    
    // Filters
    Filter(FilterType, Box<QueryExpression>),
    
    // Scoped queries
    Scope(ScopeType, Box<QueryExpression>),
}

pub enum FilterType {
    Language(String),
    Visibility(Visibility),
    IsAsync(bool),
    IsMethod(bool),
    ParameterCount(RangeFilter),
    FileSize(RangeFilter),
    LastModified(TimeFilter),
    LineRange(u32, u32),
}

pub enum ScopeType {
    File(String),
    Directory(String),
    Repository,
    Function(String),
    Struct(String),
}

pub struct RangeFilter {
    pub min: Option<u32>,
    pub max: Option<u32>,
}

pub struct TimeFilter {
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
}
```

### Query Examples
```
# Basic searches
function_name
"exact function name"
/regex_pattern/
~fuzzy_name

# Filtered searches
function_name lang:rust
async_fn is:async
public_methods is:method visibility:public
recent_changes modified:>2024-01-01

# Complex queries
(auth* OR login*) AND lang:python
functions params:>2 AND params:<=5
structs in:src/models/ visibility:public

# Scoped searches
in:src/auth.rs validate*
struct:User method:*
file:main.rs function:main
```

### Parser Implementation
```rust
impl QueryParser {
    pub fn parse(&self, query: &str) -> Result<ParsedQuery> {
        let tokens = self.tokenizer.tokenize(query)?;
        let expression = self.parse_expression(&tokens)?;
        
        Ok(ParsedQuery {
            expression,
            original_query: query.to_string(),
            parsed_at: Utc::now(),
        })
    }
    
    fn parse_expression(&self, tokens: &[Token]) -> Result<QueryExpression> {
        let mut parser = ExpressionParser::new(tokens);
        parser.parse_or_expression()
    }
}

pub struct ParsedQuery {
    pub expression: QueryExpression,
    pub original_query: String,
    pub parsed_at: DateTime<Utc>,
}

pub struct ExpressionParser<'a> {
    tokens: &'a [Token],
    position: usize,
}

impl<'a> ExpressionParser<'a> {
    fn parse_or_expression(&mut self) -> Result<QueryExpression> {
        let left = self.parse_and_expression()?;
        
        if self.current_token_is(TokenType::Or) {
            self.advance();
            let right = self.parse_or_expression()?;
            Ok(QueryExpression::Or(Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }
    
    fn parse_and_expression(&mut self) -> Result<QueryExpression> {
        let left = self.parse_not_expression()?;
        
        if self.current_token_is(TokenType::And) {
            self.advance();
            let right = self.parse_and_expression()?;
            Ok(QueryExpression::And(Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }
    
    fn parse_primary_expression(&mut self) -> Result<QueryExpression> {
        match self.current_token()?.token_type {
            TokenType::String(ref s) => {
                self.advance();
                Ok(QueryExpression::Exact(s.clone()))
            }
            TokenType::Regex(ref pattern) => {
                self.advance();
                let regex = regex::Regex::new(pattern)?;
                Ok(QueryExpression::Regex(regex))
            }
            TokenType::Fuzzy(ref pattern) => {
                self.advance();
                Ok(QueryExpression::Fuzzy {
                    pattern: pattern.clone(),
                    threshold: 0.8,
                })
            }
            TokenType::Filter(ref filter_type, ref value) => {
                self.advance();
                let filter = self.parse_filter(filter_type, value)?;
                Ok(filter)
            }
            _ => Err(QueryError::UnexpectedToken(self.current_token()?.clone()))
        }
    }
}
```

## Search Providers

### Exact Search Provider
```rust
pub struct ExactSearchProvider {
    database: Arc<DatabaseManager>,
}

impl SearchProvider for ExactSearchProvider {
    fn search(&self, query: &QueryExpression, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        match query {
            QueryExpression::Exact(pattern) => {
                self.search_exact_pattern(pattern, options)
            }
            QueryExpression::Regex(regex) => {
                self.search_regex_pattern(regex, options)
            }
            _ => Err(SearchError::UnsupportedQuery),
        }
    }
}

impl ExactSearchProvider {
    fn search_exact_pattern(&self, pattern: &str, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();
        
        // Search functions
        if options.search_functions {
            let functions = self.database.search_functions_exact(pattern, options.limit)?;
            for function in functions {
                results.push(SearchResult::Function(FunctionSearchResult {
                    function,
                    relevance_score: 1.0,
                    match_type: MatchType::Exact,
                }));
            }
        }
        
        // Search structs
        if options.search_structs {
            let structs = self.database.search_structs_exact(pattern, options.limit)?;
            for struct_def in structs {
                results.push(SearchResult::Struct(StructSearchResult {
                    struct_def,
                    relevance_score: 1.0,
                    match_type: MatchType::Exact,
                }));
            }
        }
        
        Ok(results)
    }
    
    fn search_regex_pattern(&self, regex: &regex::Regex, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        // Implementation for regex search using database LIKE queries
        // with post-filtering using regex
        Ok(vec![])
    }
}
```

### Fuzzy Search Provider
```rust
pub struct FuzzySearchProvider {
    database: Arc<DatabaseManager>,
    similarity_calculator: SimilarityCalculator,
}

impl SearchProvider for FuzzySearchProvider {
    fn search(&self, query: &QueryExpression, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        match query {
            QueryExpression::Fuzzy { pattern, threshold } => {
                self.search_fuzzy_pattern(pattern, *threshold, options)
            }
            _ => Err(SearchError::UnsupportedQuery),
        }
    }
}

impl FuzzySearchProvider {
    fn search_fuzzy_pattern(
        &self,
        pattern: &str,
        threshold: f64,
        options: &SearchOptions
    ) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();
        
        // Get all candidate names from database
        let candidates = self.database.get_all_names(options)?;
        
        // Calculate similarity scores
        for candidate in candidates {
            let similarity = self.similarity_calculator.calculate(pattern, &candidate.name);
            
            if similarity >= threshold {
                let result = SearchResult::from_candidate(candidate, similarity, MatchType::Fuzzy);
                results.push(result);
            }
        }
        
        // Sort by similarity score
        results.sort_by(|a, b| {
            b.relevance_score().partial_cmp(&a.relevance_score()).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        Ok(results)
    }
}

pub struct SimilarityCalculator {
    algorithm: SimilarityAlgorithm,
}

pub enum SimilarityAlgorithm {
    Levenshtein,
    Jaro,
    JaroWinkler,
    Jaccard,
}

impl SimilarityCalculator {
    pub fn calculate(&self, pattern: &str, candidate: &str) -> f64 {
        match self.algorithm {
            SimilarityAlgorithm::Levenshtein => {
                let distance = levenshtein_distance(pattern, candidate);
                let max_len = pattern.len().max(candidate.len()) as f64;
                1.0 - (distance as f64 / max_len)
            }
            SimilarityAlgorithm::JaroWinkler => {
                jaro_winkler_similarity(pattern, candidate)
            }
            // Other algorithms...
        }
    }
}
```

### Full-Text Search Provider
```rust
pub struct FullTextSearchProvider {
    database: Arc<DatabaseManager>,
}

impl SearchProvider for FullTextSearchProvider {
    fn search(&self, query: &QueryExpression, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        match query {
            QueryExpression::Exact(pattern) => {
                self.search_fts(pattern, options)
            }
            _ => Err(SearchError::UnsupportedQuery),
        }
    }
}

impl FullTextSearchProvider {
    fn search_fts(&self, pattern: &str, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        // Use SQLite FTS for searching across function names, documentation, etc.
        let sql = r#"
            SELECT 
                f.id, f.name, f.qualified_name, f.doc_comment,
                files.relative_path,
                rank
            FROM functions_fts fts
            JOIN functions f ON fts.rowid = f.id
            JOIN files ON f.file_id = files.id
            WHERE functions_fts MATCH ?
            ORDER BY rank
            LIMIT ?
        "#;
        
        let results = self.database.execute_query(sql, &[pattern, &options.limit.to_string()])?;
        
        // Convert database results to SearchResult objects
        Ok(self.convert_to_search_results(results)?)
    }
}
```

## Result Ranking & Scoring

### Ranking Algorithm
```rust
pub struct ResultRanker {
    ranking_factors: Vec<RankingFactor>,
    weights: RankingWeights,
}

pub struct RankingWeights {
    pub exact_match: f64,
    pub name_similarity: f64,
    pub visibility: f64,
    pub recency: f64,
    pub usage_frequency: f64,
    pub file_importance: f64,
}

pub enum RankingFactor {
    MatchType(MatchType),
    NameSimilarity(f64),
    Visibility(Visibility),
    LastModified(DateTime<Utc>),
    UsageCount(u32),
    FileImportance(f64),
}

impl ResultRanker {
    pub fn rank_results(&self, results: &mut Vec<SearchResult>) {
        for result in results.iter_mut() {
            let score = self.calculate_score(result);
            result.set_relevance_score(score);
        }
        
        // Sort by relevance score (descending)
        results.sort_by(|a, b| {
            b.relevance_score().partial_cmp(&a.relevance_score()).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    
    fn calculate_score(&self, result: &SearchResult) -> f64 {
        let mut score = 0.0;
        
        // Match type bonus
        score += match result.match_type() {
            MatchType::Exact => self.weights.exact_match,
            MatchType::Prefix => self.weights.exact_match * 0.9,
            MatchType::Substring => self.weights.exact_match * 0.7,
            MatchType::Fuzzy => self.weights.name_similarity * result.base_similarity(),
            MatchType::Regex => self.weights.exact_match * 0.8,
        };
        
        // Visibility bonus (public items are more important)
        score += match result.visibility() {
            Visibility::Public => self.weights.visibility,
            Visibility::Protected => self.weights.visibility * 0.5,
            _ => 0.0,
        };
        
        // Recency bonus
        if let Some(last_modified) = result.last_modified() {
            let days_old = (Utc::now() - last_modified).num_days() as f64;
            let recency_factor = (-days_old / 365.0).exp(); // Exponential decay
            score += self.weights.recency * recency_factor;
        }
        
        // File importance (based on file size, number of functions, etc.)
        score += self.weights.file_importance * result.file_importance();
        
        score
    }
}
```

## Filter Engine

### Advanced Filtering
```rust
pub struct FilterEngine {
    filter_processors: HashMap<FilterType, Box<dyn FilterProcessor>>,
}

pub trait FilterProcessor: Send + Sync {
    fn apply_filter(&self, results: &[SearchResult], filter_value: &str) -> Result<Vec<SearchResult>>;
}

pub struct LanguageFilter;

impl FilterProcessor for LanguageFilter {
    fn apply_filter(&self, results: &[SearchResult], language: &str) -> Result<Vec<SearchResult>> {
        Ok(results
            .iter()
            .filter(|result| result.file_language().as_deref() == Some(language))
            .cloned()
            .collect())
    }
}

pub struct VisibilityFilter;

impl FilterProcessor for VisibilityFilter {
    fn apply_filter(&self, results: &[SearchResult], visibility: &str) -> Result<Vec<SearchResult>> {
        let target_visibility = Visibility::from_str(visibility)?;
        Ok(results
            .iter()
            .filter(|result| result.visibility() == target_visibility)
            .cloned()
            .collect())
    }
}

pub struct ParameterCountFilter;

impl FilterProcessor for ParameterCountFilter {
    fn apply_filter(&self, results: &[SearchResult], range: &str) -> Result<Vec<SearchResult>> {
        let range_filter = RangeFilter::parse(range)?;
        
        Ok(results
            .iter()
            .filter(|result| {
                if let Some(param_count) = result.parameter_count() {
                    range_filter.contains(param_count)
                } else {
                    false
                }
            })
            .cloned()
            .collect())
    }
}

impl RangeFilter {
    pub fn parse(range_str: &str) -> Result<Self> {
        // Parse ranges like ">5", "<=10", "3..7", "5"
        if range_str.starts_with(">=") {
            Ok(RangeFilter {
                min: Some(range_str[2..].parse()?),
                max: None,
            })
        } else if range_str.starts_with('>') {
            Ok(RangeFilter {
                min: Some(range_str[1..].parse::<u32>()? + 1),
                max: None,
            })
        } else if range_str.starts_with("<=") {
            Ok(RangeFilter {
                min: None,
                max: Some(range_str[2..].parse()?),
            })
        } else if range_str.starts_with('<') {
            Ok(RangeFilter {
                min: None,
                max: Some(range_str[1..].parse::<u32>()? - 1),
            })
        } else if range_str.contains("..") {
            let parts: Vec<&str> = range_str.split("..").collect();
            Ok(RangeFilter {
                min: Some(parts[0].parse()?),
                max: Some(parts[1].parse()?),
            })
        } else {
            let value = range_str.parse()?;
            Ok(RangeFilter {
                min: Some(value),
                max: Some(value),
            })
        }
    }
    
    pub fn contains(&self, value: u32) -> bool {
        if let Some(min) = self.min {
            if value < min {
                return false;
            }
        }
        if let Some(max) = self.max {
            if value > max {
                return false;
            }
        }
        true
    }
}
```

## Search Results

### Result Types
```rust
#[derive(Debug, Clone)]
pub enum SearchResult {
    Function(FunctionSearchResult),
    Struct(StructSearchResult),
    Import(ImportSearchResult),
    Export(ExportSearchResult),
    File(FileSearchResult),
}

#[derive(Debug, Clone)]
pub struct FunctionSearchResult {
    pub function: FunctionSignature,
    pub file_path: String,
    pub relevance_score: f64,
    pub match_type: MatchType,
    pub match_context: MatchContext,
}

#[derive(Debug, Clone)]
pub struct StructSearchResult {
    pub struct_def: StructSignature,
    pub file_path: String,
    pub relevance_score: f64,
    pub match_type: MatchType,
    pub match_context: MatchContext,
}

#[derive(Debug, Clone)]
pub struct MatchContext {
    pub matched_field: MatchedField,
    pub snippet: Option<String>,
    pub line_number: u32,
}

#[derive(Debug, Clone)]
pub enum MatchedField {
    Name,
    QualifiedName,
    Documentation,
    Parameter,
    ReturnType,
    FieldName,
    FieldType,
}

#[derive(Debug, Clone)]
pub enum MatchType {
    Exact,
    Prefix,
    Substring,
    Fuzzy,
    Regex,
}

impl SearchResult {
    pub fn relevance_score(&self) -> f64 {
        match self {
            SearchResult::Function(f) => f.relevance_score,
            SearchResult::Struct(s) => s.relevance_score,
            SearchResult::Import(i) => i.relevance_score,
            SearchResult::Export(e) => e.relevance_score,
            SearchResult::File(f) => f.relevance_score,
        }
    }
    
    pub fn file_path(&self) -> &str {
        match self {
            SearchResult::Function(f) => &f.file_path,
            SearchResult::Struct(s) => &s.file_path,
            SearchResult::Import(i) => &i.file_path,
            SearchResult::Export(e) => &e.file_path,
            SearchResult::File(f) => &f.file_path,
        }
    }
    
    pub fn name(&self) -> &str {
        match self {
            SearchResult::Function(f) => &f.function.name,
            SearchResult::Struct(s) => &s.struct_def.name,
            SearchResult::Import(i) => &i.import.imported_name,
            SearchResult::Export(e) => &e.export.exported_name,
            SearchResult::File(f) => &f.file_name,
        }
    }
}
```

## Query Cache

### Caching System
```rust
pub struct QueryCache {
    cache: Arc<Mutex<HashMap<String, CachedResult>>>,
    max_size: usize,
    ttl: Duration,
}

pub struct CachedResult {
    pub results: Vec<SearchResult>,
    pub created_at: Instant,
    pub hit_count: u32,
}

impl QueryCache {
    pub fn get(&self, query_hash: &str) -> Option<Vec<SearchResult>> {
        let mut cache = self.cache.lock().unwrap();
        
        if let Some(cached) = cache.get_mut(query_hash) {
            if cached.created_at.elapsed() < self.ttl {
                cached.hit_count += 1;
                return Some(cached.results.clone());
            } else {
                // Remove expired entry
                cache.remove(query_hash);
            }
        }
        
        None
    }
    
    pub fn insert(&self, query_hash: String, results: Vec<SearchResult>) {
        let mut cache = self.cache.lock().unwrap();
        
        // Evict least recently used entries if cache is full
        if cache.len() >= self.max_size {
            self.evict_lru(&mut cache);
        }
        
        cache.insert(query_hash, CachedResult {
            results,
            created_at: Instant::now(),
            hit_count: 0,
        });
    }
    
    fn evict_lru(&self, cache: &mut HashMap<String, CachedResult>) {
        // Find entry with lowest hit count and oldest creation time
        let mut oldest_key = None;
        let mut oldest_score = f64::MAX;
        
        for (key, cached) in cache.iter() {
            let age_score = cached.created_at.elapsed().as_secs() as f64;
            let hit_score = 1.0 / (cached.hit_count as f64 + 1.0);
            let score = age_score + hit_score * 100.0; // Bias towards evicting low-hit entries
            
            if score < oldest_score {
                oldest_score = score;
                oldest_key = Some(key.clone());
            }
        }
        
        if let Some(key) = oldest_key {
            cache.remove(&key);
        }
    }
}

impl QueryEngine {
    fn calculate_query_hash(query: &ParsedQuery, options: &SearchOptions) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        query.original_query.hash(&mut hasher);
        options.hash(&mut hasher);
        
        format!("{:x}", hasher.finish())
    }
}
```

## Performance Optimization

### Parallel Search
```rust
impl QueryEngine {
    pub async fn search_parallel(&self, query: &ParsedQuery, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        let query_hash = Self::calculate_query_hash(query, options);
        
        // Check cache first
        if let Some(cached_results) = self.cache.get(&query_hash) {
            return Ok(cached_results);
        }
        
        // Execute searches in parallel across different backends
        let mut search_tasks = Vec::new();
        
        for (backend, provider) in &self.search_backends {
            if self.should_use_backend(backend, &query.expression) {
                let provider = provider.clone();
                let query_expr = query.expression.clone();
                let search_options = options.clone();
                
                let task = tokio::spawn(async move {
                    provider.search(&query_expr, &search_options).await
                });
                
                search_tasks.push(task);
            }
        }
        
        // Collect results from all backends
        let mut all_results = Vec::new();
        for task in search_tasks {
            match task.await? {
                Ok(mut results) => all_results.append(&mut results),
                Err(e) => eprintln!("Search backend error: {}", e),
            }
        }
        
        // Remove duplicates and rank results
        let mut deduplicated = self.deduplicate_results(all_results);
        self.result_ranker.rank_results(&mut deduplicated);
        
        // Apply limit
        deduplicated.truncate(options.limit);
        
        // Cache results
        self.cache.insert(query_hash, deduplicated.clone());
        
        Ok(deduplicated)
    }
    
    fn deduplicate_results(&self, results: Vec<SearchResult>) -> Vec<SearchResult> {
        let mut seen = HashSet::new();
        let mut deduplicated = Vec::new();
        
        for result in results {
            let key = self.result_key(&result);
            if !seen.contains(&key) {
                seen.insert(key);
                deduplicated.push(result);
            }
        }
        
        deduplicated
    }
    
    fn result_key(&self, result: &SearchResult) -> String {
        format!("{}:{}:{}", 
            result.result_type(),
            result.file_path(),
            result.name()
        )
    }
}
```

## Error Handling

### Query Errors
```rust
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("Invalid query syntax: {0}")]
    InvalidSyntax(String),
    
    #[error("Unsupported query expression")]
    UnsupportedExpression,
    
    #[error("Regex compilation error: {0}")]
    RegexError(#[from] regex::Error),
    
    #[error("Filter parsing error: {0}")]
    FilterError(String),
    
    #[error("Search timeout")]
    Timeout,
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Cache error: {0}")]
    CacheError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Unsupported query type for this backend")]
    UnsupportedQuery,
    
    #[error("Backend unavailable: {0}")]
    BackendUnavailable(String),
    
    #[error("Search index corrupted")]
    IndexCorrupted,
    
    #[error("Resource limit exceeded")]
    ResourceLimitExceeded,
}
```

## Configuration

### Query Engine Configuration
```toml
[query_engine]
default_max_results = 50
fuzzy_threshold = 0.7
enable_caching = true
cache_ttl_seconds = 300
parallel_search = true
search_timeout_ms = 5000

[search_backends]
exact = { enabled = true, priority = 1 }
fuzzy = { enabled = true, priority = 2, threshold = 0.7 }
full_text = { enabled = true, priority = 3 }
semantic = { enabled = false, priority = 4 }

[ranking]
exact_match_weight = 10.0
name_similarity_weight = 5.0
visibility_weight = 2.0
recency_weight = 1.0
usage_frequency_weight = 3.0
file_importance_weight = 1.5

[cache]
max_size = 1000
eviction_policy = "lru"
cleanup_interval_seconds = 60
```

## Testing

### Unit Tests
- Query parsing accuracy
- Search result ranking
- Filter application
- Cache behavior

### Performance Tests
- Large dataset search performance
- Parallel search efficiency
- Memory usage under load
- Cache hit rate optimization

### Integration Tests
- End-to-end search workflows
- Multiple backend coordination
- Complex query evaluation
- Error recovery scenarios 