from pathlib import Path
p = Path(r'crates/core/src/storage/model_catalog_v2.rs')
t = p.read_text(encoding='utf-8')
if 'use crate::storage::AggregateApi;' not in t:
    t = t.replace(
        'mod tests {\n    use super::*;\n',
        'mod tests {\n    use super::*;\n    use crate::storage::AggregateApi;\n',
        1,
    )
old = '''        grok.price = ModelPriceV2 {
            currency: "USD".to_string(),
            input_microusd_per_1m: None,
            cached_input_microusd_per_1m: None,
            output_microusd_per_1m: None,
            price_status: "missing".to_string(),
            price_source: None,
        };'''
new = '''        grok.price = ModelPriceV2 {
            input_microusd_per_1m: None,
            cached_input_microusd_per_1m: None,
            output_microusd_per_1m: None,
            price_status: "missing".to_string(),
            price_source: None,
        };'''
if old in t:
    t = t.replace(old, new, 1)
    print('price fixed')
else:
    i = t.find('grok.price = ModelPriceV2')
    print('price block missing at', i)
    print(repr(t[i:i+300]))
print('has import', 'use crate::storage::AggregateApi;' in t)
p.write_text(t, encoding='utf-8')
