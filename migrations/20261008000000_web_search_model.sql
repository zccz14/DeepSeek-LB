-- The web search host model: one `/v1/web-search` call runs one
-- Anthropic-compatible Messages turn on this model, so which model serves a
-- search is a deployment setting rather than a caller argument. `deepseek-flash`
-- is the cheapest DeepSeek model and the one the price table already lists.
INSERT INTO app_meta(key,value,updated_at)
VALUES ('web_search_model','deepseek-flash',unixepoch());
