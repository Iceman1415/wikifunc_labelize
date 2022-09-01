## Step 0: json

The input is simply a json, from `serde_json` crate

```rust
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Map<String, Value>),
}
```

## Step 1

First of all, we drop the Null, Bool, and Number variants, as they shouldn't be in our json.

We then also fetch the human readable labels for all ZIDs and Keys (in the form ZxxKyy), and transform them from a simple string into a LabelledNode.

```rust
struct LabelledNode {
    readable_labels: BTreeSet<(String, String)>,
    z_label: String,
}

enum StringType {
    String(String),
    LabelledNode(LabelledNode),
}

enum SimpleValue {
    StringType(StringType),
    Array(Vec<SimpleValue>),
    Object(BTreeSet<(StringType, SimpleValue)>),
}
```

## Step 2

All ZObjects contains the key Z1K1, with the ZObject's type as the corresponding value. The first (0th) item of an array in a ZObject is the type of the array.

We would like to "lift" both of these kinds of types "upward", separating them from the rest of the content.

A Type can just be a string (which we may have converted into a LabelledString), or it can be an Zobject. If the Type is an Zobject, the Type Zobject should have contains the key Z1K1,

```rust
enum Type {
    Simple(StringType),
    WithArgs(StringType, BTreeSet<(StringType, SimpleValue)>),
}

enum CompactKey {
    StringType(StringType),
    TypedLabelledNode(StringType, SimpleType),
    Transient(SimpleType),
}

enum IntermediateForm {
    KeyType(CompactKey),
    Array(Type, Vec<IntermediateForm>),
    Object(BTreeSet<(StringType, IntermediateForm)>),
    // in the intermediate form, we pull the type of objects out
    TypedObject(Type, BTreeSet<(StringType, IntermediateForm)>),
}
```

## 

```rust
enum CompactValue {
    KeyType(CompactKey),
    Array(Vec<CompactValue>),
    Object(BTreeSet<(CompactKey, CompactValue)>),
}
```
