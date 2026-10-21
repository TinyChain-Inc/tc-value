use super::*;
use get_size::GetSize;

#[test]
fn retained_size_includes_inline_values_and_spare_nested_capacity() {
    let mut string = String::with_capacity(64);
    string.push_str("abc");
    let string_capacity = string.capacity();
    let mut values = Vec::with_capacity(8);
    values.push(Value::default());
    values.push(Value::from(string));
    let capacity = values.capacity();
    assert_eq!(Value::default().get_size(), std::mem::size_of::<Value>());
    assert_eq!(
        Value::from(values).get_size(),
        std::mem::size_of::<Value>() + capacity * std::mem::size_of::<Value>() + string_capacity
    );
    let link: Link = "https://example.com/one/two/three".parse().unwrap();
    assert!(link.get_heap_size() > 0);
    assert!(Value::from(link).get_size() > std::mem::size_of::<Value>());
}
