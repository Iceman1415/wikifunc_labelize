## Type Convertions

To "compactify" an ZObject, we convert the input `Value` into an output `Value`, by converting the value through multiple stages. Each convertion should do minimal amount of work, only doing one or two tasks, and be easy to read and debug separately.

0. json `Value`, from `serde_json`
    * this is the payload; the http POST request body
1. `SimpleValue`: [simple_value.rs](./src/simple_value.rs)
    * We fetch data about ZIDs (Zxxx) and global Keys (ZxxxKyyy) from wikifunction api, and convert `Value::String(String)` into `LabelledNode`, if possible.
    * We drop unused variants of `Value` (`Value::Null`, `Value::Bool`, `Value::Number`).
2. `TypedValue`: [typed_value.rs](./src/typed_value.rs)
    * We separate type information from the rest of the data
    * `SimpleValue::Object(obj)` becomes `TypedValue::TypedObject(typ, obj)` if possible.
    * `SimpleValue::Array(arr)` becomes `TypedValue::TypedArray(typ, arr)` if possible.
3. `IntermediateForm`: [intermediate_form.rs](./src/intermediate_form.rs)
    * There are move valid values in this type compared to `TypedValue`, making transformations `fn (IntermediateForm) -> IntermediateForm` easier.
    * Additional LabelledNode variant, used in .compress_monolingual()
    * ...more to be added?
4. `CompactValue`: [compact_value.rs](./src/compact_value.rs)
    * We push the type information into the parent object's key.
    * `CompactValue` is very similar to `SimpleValue`, and easy to convert into json `Value`
