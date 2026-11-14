use crate::ffi::ffi;
use crate::ffi::ffi::PhysicalTypeID;
use crate::logical_type::LogicalType;
use rust_decimal::prelude::ToPrimitive;
use std::cmp::Ordering;
use std::convert::{TryFrom, TryInto};
use std::fmt;

#[non_exhaustive]
pub enum ConversionError {
    /// Lbug's internal date as the number of days since 1970-01-01
    Date(i32),
    /// Lbug's internal timestamp as the number of microseconds since 1970-01-01
    Timestamp(i64),
    TimestampTz(i64),
    TimestampNs(i64),
    TimestampMs(i64),
    TimestampSec(i64),
    Json(String, serde_json::Error),
    /// Lbug's internal decimal as its scaled integer and scale, when it
    /// exceeds what `rust_decimal` can represent
    Decimal(i128, u32, rust_decimal::Error),
}

impl std::fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::fmt::Debug for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ConversionError::{
            Date, Decimal, Json, Timestamp, TimestampMs, TimestampNs, TimestampSec, TimestampTz,
        };
        match self {
            Date(days) => write!(f, "Could not convert Lbug date offset of UNIX_EPOCH + {days} days to time::Date"),
            Timestamp(us) => write!(f, "Could not convert Lbug timestamp offset of UNIX_EPOCH + {us} microseconds to time::OffsetDateTime"),
            TimestampTz(us) => write!(f, "Could not convert Lbug timestamp_tz offset of UNIX_EPOCH + {us} microseconds to time::OffsetDateTime"),
            TimestampNs(ns) => write!(f, "Could not convert Lbug timestamp_ns offset of UNIX_EPOCH + {ns} nanoseconds to time::OffsetDateTime"),
            TimestampMs(ms) => write!(f, "Could not convert Lbug timestamp_ms offset of UNIX_EPOCH + {ms} milliseconds to time::OffsetDateTime"),
            TimestampSec(sec) => write!(f, "Could not convert Lbug timestamp_sec offset of UNIX_EPOCH + {sec} seconds to time::OffsetDateTime"),
            Json(value, err) => write!(f, "Could not convert Lbug JSON value {value:?}: {err}"),
            Decimal(value, scale, err) => write!(
                f,
                "Could not convert Lbug decimal value {value} with scale {scale} to rust_decimal::Decimal: {err}"
            ),
        }
    }
}

impl std::error::Error for ConversionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConversionError::Decimal(_, _, err) => Some(err),
            ConversionError::Json(_, err) => Some(err),
            _ => None,
        }
    }
}

/// `NodeVal` represents a node in the graph and stores the nodeID, label and properties of that
/// node.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeVal {
    id: InternalID,
    label: String,
    properties: Vec<(String, Value)>,
}

impl NodeVal {
    pub fn new<I: Into<InternalID>, S: Into<String>>(id: I, label: S) -> Self {
        NodeVal {
            id: id.into(),
            label: label.into(),
            properties: vec![],
        }
    }

    pub fn get_node_id(&self) -> &InternalID {
        &self.id
    }

    pub fn get_label_name(&self) -> &String {
        &self.label
    }

    /// Adds a property with the given key/value pair to the `NodeVal`
    /// # Arguments
    /// * `key`: The name of the property
    /// * `value`: The value of the property
    pub fn add_property<S: Into<String>, V: Into<Value>>(&mut self, key: S, value: V) {
        self.properties.push((key.into(), value.into()));
    }

    /// Returns all properties of the `NodeVal`
    pub fn get_properties(&self) -> &Vec<(String, Value)> {
        &self.properties
    }
}

fn properties_display(f: &mut fmt::Formatter<'_>, properties: &[(String, Value)]) -> fmt::Result {
    write!(f, "{{")?;
    for (index, (name, value)) in properties.iter().enumerate() {
        write!(f, "{name}:{value}")?;
        if index < properties.len() - 1 {
            write!(f, ",")?;
        }
    }
    write!(f, "}}")
}

impl std::fmt::Display for NodeVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(label:{}, {}, ", self.label, self.id)?;
        properties_display(f, &self.properties)?;
        write!(f, ")")
    }
}

/// `RelVal` represents a relationship in the graph and stores the relID, src/dst nodes and properties of that
/// rel
#[derive(Clone, Debug, PartialEq)]
pub struct RelVal {
    src_node: InternalID,
    dst_node: InternalID,
    label: String,
    properties: Vec<(String, Value)>,
}

impl RelVal {
    pub fn new<I: Into<InternalID>, S: Into<String>>(src_node: I, dst_node: I, label: S) -> Self {
        RelVal {
            src_node: src_node.into(),
            dst_node: dst_node.into(),
            label: label.into(),
            properties: vec![],
        }
    }

    pub fn get_src_node(&self) -> &InternalID {
        &self.src_node
    }
    pub fn get_dst_node(&self) -> &InternalID {
        &self.dst_node
    }

    pub fn get_label_name(&self) -> &String {
        &self.label
    }

    /// Adds a property with the given key/value pair to the `NodeVal`
    /// # Arguments
    /// * `key`: The name of the property
    /// * `value`: The value of the property
    pub fn add_property(&mut self, key: String, value: Value) {
        self.properties.push((key, value));
    }

    /// Returns all properties of the `RelVal`
    pub fn get_properties(&self) -> &Vec<(String, Value)> {
        &self.properties
    }
}

impl std::fmt::Display for RelVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({})-[label:{}, ", self.src_node, self.label)?;
        properties_display(f, &self.properties)?;
        write!(f, "]->({})", self.dst_node)
    }
}

/// Stores the `table_id` and `offset` of a node/rel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InternalID {
    pub offset: u64,
    pub table_id: u64,
}

impl std::fmt::Display for InternalID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.table_id, self.offset)
    }
}

impl PartialOrd for InternalID {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InternalID {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.table_id == other.table_id {
            self.offset.cmp(&other.offset)
        } else {
            self.table_id.cmp(&other.table_id)
        }
    }
}

impl From<(u64, u64)> for InternalID {
    fn from(value: (u64, u64)) -> Self {
        InternalID {
            offset: value.0,
            table_id: value.1,
        }
    }
}

/// Data types supported by Lbug
///
/// Also see <https://ladybugdb.com/docusaurus/cypher/data-types/overview.html>
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null(LogicalType),
    Bool(bool),
    Int64(i64),
    Int32(i32),
    Int16(i16),
    Int8(i8),
    UInt64(u64),
    UInt32(u32),
    UInt16(u16),
    UInt8(u8),
    Int128(i128),
    Double(f64),
    Float(f32),
    /// Stored internally as the number of days since 1970-01-01 as a 32-bit signed integer, which
    /// allows for a wider range of dates to be stored than can be represented by `time::Date`
    ///
    /// <https://ladybugdb.com/docusaurus/cypher/data-types/date.html>
    Date(time::Date),
    /// May be signed or unsigned.
    ///
    /// Nanosecond precision of `time::Duration` (if available) will not be preserved when passed to
    /// queries, and results will always have at most microsecond precision.
    ///
    /// <https://ladybugdb.com/docusaurus/cypher/data-types/interval.html>
    Interval(time::Duration),
    /// Stored internally as the number of microseconds since 1970-01-01
    /// Nanosecond precision of `SystemTime` (if available) will not be preserved when used.
    ///
    /// <https://ladybugdb.com/docusaurus/cypher/data-types/timestamp.html>
    Timestamp(time::OffsetDateTime),
    TimestampTz(time::OffsetDateTime),
    TimestampNs(time::OffsetDateTime),
    TimestampMs(time::OffsetDateTime),
    TimestampSec(time::OffsetDateTime),
    InternalID(InternalID),
    /// <https://ladybugdb.com/docusaurus/cypher/data-types/string.html>
    String(String),
    Json(serde_json::Value),
    Blob(Vec<u8>),
    // TODO: Enforce type of contents
    // LogicalType is necessary so that we can pass the correct type to the C++ API if the list is empty.
    /// These must contain elements which are all the given type.
    /// <https://ladybugdb.com/docusaurus/cypher/data-types/list.html>
    List(LogicalType, Vec<Value>),
    /// These must contain elements which are all the same type.
    /// <https://ladybugdb.com/docusaurus/cypher/data-types/list.html>
    Array(LogicalType, Vec<Value>),
    /// <https://ladybugdb.com/docusaurus/cypher/data-types/struct.html>
    Struct(Vec<(String, Value)>),
    Node(NodeVal),
    Rel(RelVal),
    RecursiveRel {
        /// Interior nodes in the Sequence of Rels
        ///
        /// Does not include the starting or ending Node.
        nodes: Vec<NodeVal>,
        /// Sequence of Rels which make up the `RecursiveRel`
        rels: Vec<RelVal>,
    },
    /// <https://ladybugdb.com/docusaurus/cypher/data-types/map>
    Map((LogicalType, LogicalType), Vec<(Value, Value)>),
    /// <https://ladybugdb.com/docusaurus/cypher/data-types/union>
    Union {
        types: Vec<(String, LogicalType)>,
        value: Box<Value>,
    },
    UUID(uuid::Uuid),
    Decimal(rust_decimal::Decimal),
}

fn display_list<T: std::fmt::Display>(f: &mut fmt::Formatter<'_>, list: &[T]) -> fmt::Result {
    write!(f, "[")?;
    for (i, value) in list.iter().enumerate() {
        write!(f, "{value}")?;
        if i != list.len() - 1 {
            write!(f, ",")?;
        }
    }
    write!(f, "]")
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(true) => write!(f, "True"),
            Value::Bool(false) => write!(f, "False"),
            Value::Int8(x) => write!(f, "{x}"),
            Value::Int16(x) => write!(f, "{x}"),
            Value::Int32(x) => write!(f, "{x}"),
            Value::Int64(x) => write!(f, "{x}"),
            Value::UInt8(x) => write!(f, "{x}"),
            Value::UInt16(x) => write!(f, "{x}"),
            Value::UInt32(x) => write!(f, "{x}"),
            Value::UInt64(x) => write!(f, "{x}"),
            Value::Int128(x) => write!(f, "{x}"),
            Value::Date(x) => write!(f, "{x}"),
            Value::String(x) => write!(f, "{x}"),
            Value::Json(x) => write!(f, "{x}"),
            Value::Blob(x) => write!(f, "{x:x?}"),
            Value::Null(_) => write!(f, ""),
            Value::List(_, x) | Value::Array(_, x) => display_list(f, x),
            // Note: These don't match lbug's toString, but we probably don't want them to
            Value::Interval(x) => write!(f, "{x}"),
            Value::Timestamp(x)
            | Value::TimestampTz(x)
            | Value::TimestampNs(x)
            | Value::TimestampMs(x)
            | Value::TimestampSec(x) => write!(f, "{x}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::Double(x) => write!(f, "{x}"),
            Value::Struct(x) => {
                write!(f, "{{")?;
                for (i, (name, value)) in x.iter().enumerate() {
                    write!(f, "{name}: {value}")?;
                    if i != x.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "}}")
            }
            Value::Map(_, x) => {
                write!(f, "{{")?;
                for (i, (name, value)) in x.iter().enumerate() {
                    write!(f, "{name}={value}")?;
                    if i != x.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "}}")
            }
            Value::Node(x) => write!(f, "{x}"),
            Value::Rel(x) => write!(f, "{x}"),
            Value::InternalID(x) => write!(f, "{x}"),
            Value::RecursiveRel { nodes, rels } => {
                write!(f, "{{")?;
                write!(f, "_NODES: ")?;
                display_list(f, nodes)?;
                write!(f, ", _RELS: ")?;
                display_list(f, rels)?;
                write!(f, "}}")
            }
            Value::Union { types: _, value } => write!(f, "{value}"),
            Value::UUID(x) => write!(f, "{x}"),
            Value::Decimal(value) => write!(f, "{value}"),
        }
    }
}

impl From<&Value> for LogicalType {
    fn from(value: &Value) -> Self {
        match value {
            Value::Bool(_) => LogicalType::Bool,
            Value::Int8(_) => LogicalType::Int8,
            Value::Int16(_) => LogicalType::Int16,
            Value::Int32(_) => LogicalType::Int32,
            Value::Int64(_) => LogicalType::Int64,
            Value::UInt8(_) => LogicalType::UInt8,
            Value::UInt16(_) => LogicalType::UInt16,
            Value::UInt32(_) => LogicalType::UInt32,
            Value::UInt64(_) => LogicalType::UInt64,
            Value::Int128(_) => LogicalType::Int128,
            Value::Float(_) => LogicalType::Float,
            Value::Double(_) => LogicalType::Double,
            Value::Date(_) => LogicalType::Date,
            Value::Interval(_) => LogicalType::Interval,
            Value::Timestamp(_) => LogicalType::Timestamp,
            Value::TimestampTz(_) => LogicalType::TimestampTz,
            Value::TimestampNs(_) => LogicalType::TimestampNs,
            Value::TimestampMs(_) => LogicalType::TimestampMs,
            Value::TimestampSec(_) => LogicalType::TimestampSec,
            Value::String(_) => LogicalType::String,
            Value::Json(_) => LogicalType::Json,
            Value::Blob(_) => LogicalType::Blob,
            Value::Null(x) => x.clone(),
            Value::List(x, _) => LogicalType::List {
                child_type: Box::new(x.clone()),
            },
            Value::Array(x, value) => LogicalType::Array {
                child_type: Box::new(x.clone()),
                num_elements: value.len() as u64,
            },
            Value::Struct(values) => LogicalType::Struct {
                fields: values
                    .iter()
                    .map(|(name, x)| {
                        let typ: LogicalType = x.into();
                        (name.clone(), typ)
                    })
                    .collect(),
            },
            Value::InternalID(_) => LogicalType::InternalID,
            Value::Node(_) => LogicalType::Node,
            Value::Rel(_) => LogicalType::Rel,
            Value::RecursiveRel { .. } => LogicalType::RecursiveRel,
            Value::Map((key_type, value_type), _) => LogicalType::Map {
                key_type: Box::new(key_type.clone()),
                value_type: Box::new(value_type.clone()),
            },
            Value::Union { types, value: _ } => LogicalType::Union {
                types: types.clone(),
            },
            Value::UUID(_) => LogicalType::UUID,
            Value::Decimal(value) => {
                let scale = value.scale();
                // Digits of the mantissa's magnitude; 0 has one digit.
                // unsigned_abs() is exact for every rust_decimal mantissa
                // (|mantissa| <= 2^96 - 1, far from i128::MIN).
                let digits = value
                    .mantissa()
                    .unsigned_abs()
                    .checked_ilog10()
                    .unwrap_or(0)
                    + 1;
                // Note on `.min(38)`: For rust_decimal, mantissa is at most 2^96 - 1 (~7.92e28, <=29 digits)
                // and scale is at most 28. Therefore `digits.max(scale)` never exceeds 29.
                // `.min(38)` is strictly unreachable for valid rust_decimal values, but retained as defense-in-depth.
                LogicalType::Decimal {
                    scale,
                    precision: digits.max(scale).min(38),
                }
            }
        }
    }
}

fn get_date_from_unix_days(days: i32) -> Result<time::Date, ConversionError> {
    time::Date::from_calendar_date(1970, time::Month::January, 1)
        .unwrap()
        .checked_add(time::Duration::days(i64::from(days)))
        .ok_or(ConversionError::Date(days))
}

fn get_timestamp_from_unix_micros(us: i64) -> Result<time::OffsetDateTime, ConversionError> {
    time::OffsetDateTime::UNIX_EPOCH
        .checked_add(time::Duration::microseconds(us))
        .ok_or(ConversionError::Timestamp(us))
}

impl TryFrom<&ffi::Value> for Value {
    type Error = ConversionError;

    fn try_from(value: &ffi::Value) -> Result<Self, Self::Error> {
        use ffi::LogicalTypeID;
        #[allow(clippy::cast_possible_wrap)]
        fn get_i128(value: &ffi::Value) -> i128 {
            let int128_val = ffi::value_get_int128_t(value);
            let low = int128_val[1];
            let high = int128_val[0] as i64;
            (i128::from(low)) + ((i128::from(high)) << 64)
        }

        if ffi::value_is_null(value) {
            return Ok(Value::Null(value.into()));
        }

        match ffi::value_get_data_type_id(value) {
            LogicalTypeID::ANY => unimplemented!(),
            LogicalTypeID::BOOL => Ok(Value::Bool(ffi::value_get_bool(value))),
            LogicalTypeID::INT8 => Ok(Value::Int8(ffi::value_get_i8(value))),
            LogicalTypeID::INT16 => Ok(Value::Int16(ffi::value_get_i16(value))),
            LogicalTypeID::INT32 => Ok(Value::Int32(ffi::value_get_i32(value))),
            LogicalTypeID::INT64 => Ok(Value::Int64(ffi::value_get_i64(value))),
            LogicalTypeID::UINT8 => Ok(Value::UInt8(ffi::value_get_u8(value))),
            LogicalTypeID::UINT16 => Ok(Value::UInt16(ffi::value_get_u16(value))),
            LogicalTypeID::UINT32 => Ok(Value::UInt32(ffi::value_get_u32(value))),
            LogicalTypeID::UINT64 => Ok(Value::UInt64(ffi::value_get_u64(value))),
            LogicalTypeID::INT128 => Ok(Value::Int128(get_i128(value))),
            #[allow(clippy::cast_sign_loss)]
            LogicalTypeID::UUID => Ok(Value::UUID(uuid::Uuid::from_u128(
                // values are stored as i128 and the first bit flipped so that they order as if
                // they are u128
                get_i128(value) as u128 ^ (1 << 127),
            ))),
            LogicalTypeID::FLOAT => Ok(Value::Float(ffi::value_get_float(value))),
            LogicalTypeID::DOUBLE => Ok(Value::Double(ffi::value_get_double(value))),
            LogicalTypeID::STRING => Ok(Value::String(ffi::value_get_string(value).to_string())),
            LogicalTypeID::JSON => {
                let json = ffi::value_get_string(value).to_string();
                serde_json::from_str(&json)
                    .map(Value::Json)
                    .map_err(|err| ConversionError::Json(json, err))
            }
            LogicalTypeID::BLOB => Ok(Value::Blob(
                ffi::value_get_string(value).as_bytes().to_vec(),
            )),
            LogicalTypeID::INTERVAL => Ok(Value::Interval(time::Duration::new(
                ffi::value_get_interval_secs(value),
                // Duration is constructed using nanoseconds, but lbug stores microseconds
                ffi::value_get_interval_micros(value) * 1000,
            ))),
            LogicalTypeID::DATE => Ok(Value::Date(get_date_from_unix_days(
                ffi::value_get_date_days(value),
            )?)),
            LogicalTypeID::TIMESTAMP => Ok(Value::Timestamp(get_timestamp_from_unix_micros(
                ffi::value_get_timestamp_micros(value),
            )?)),
            LogicalTypeID::TIMESTAMP_TZ => {
                let us = ffi::value_get_timestamp_tz(value);
                time::OffsetDateTime::UNIX_EPOCH
                    .checked_add(time::Duration::microseconds(us))
                    .map(Value::TimestampTz)
                    .ok_or(ConversionError::TimestampTz(us))
            }
            LogicalTypeID::TIMESTAMP_NS => {
                let ns = ffi::value_get_timestamp_ns(value);
                time::OffsetDateTime::UNIX_EPOCH
                    .checked_add(time::Duration::nanoseconds(ns))
                    .map(Value::TimestampNs)
                    .ok_or(ConversionError::TimestampNs(ns))
            }
            LogicalTypeID::TIMESTAMP_MS => {
                let ms = ffi::value_get_timestamp_ms(value);
                time::OffsetDateTime::UNIX_EPOCH
                    .checked_add(time::Duration::milliseconds(ms))
                    .map(Value::TimestampMs)
                    .ok_or(ConversionError::TimestampMs(ms))
            }
            LogicalTypeID::TIMESTAMP_SEC => {
                let sec = ffi::value_get_timestamp_sec(value);
                time::OffsetDateTime::UNIX_EPOCH
                    .checked_add(time::Duration::seconds(sec))
                    .map(Value::TimestampSec)
                    .ok_or(ConversionError::TimestampSec(sec))
            }
            LogicalTypeID::LIST => {
                let mut result = vec![];
                for index in 0..ffi::value_get_children_size(value) {
                    let value: Value = ffi::value_get_child(value, index).try_into()?;
                    result.push(value);
                }
                if let LogicalType::List { child_type } = value.into() {
                    Ok(Value::List(*child_type, result))
                } else {
                    unreachable!()
                }
            }
            LogicalTypeID::ARRAY => {
                let mut result = vec![];
                for index in 0..ffi::value_get_children_size(value) {
                    let value: Value = ffi::value_get_child(value, index).try_into()?;
                    result.push(value);
                }
                if let LogicalType::Array { child_type, .. } = value.into() {
                    Ok(Value::Array(*child_type, result))
                } else {
                    unreachable!()
                }
            }
            LogicalTypeID::STRUCT => {
                // Data is a list of field values in the value itself (same as list),
                // with the field names stored in the DataType
                let field_names =
                    ffi::logical_type_get_struct_field_names(ffi::value_get_data_type(value));
                let mut result = vec![];
                for (name, index) in field_names
                    .into_iter()
                    .zip(0..ffi::value_get_children_size(value))
                {
                    let value: Value = ffi::value_get_child(value, index).try_into()?;
                    result.push((name, value));
                }
                Ok(Value::Struct(result))
            }
            LogicalTypeID::MAP => {
                let mut result = vec![];
                for index in 0..ffi::value_get_children_size(value) {
                    let pair = ffi::value_get_child(value, index);
                    result.push((
                        ffi::value_get_child(pair, 0).try_into()?,
                        ffi::value_get_child(pair, 1).try_into()?,
                    ));
                }
                if let LogicalType::Map {
                    key_type,
                    value_type,
                } = value.into()
                {
                    Ok(Value::Map((*key_type, *value_type), result))
                } else {
                    unreachable!()
                }
            }
            LogicalTypeID::NODE => {
                let id = ffi::node_value_get_node_id(value);
                if ffi::value_is_null(id) {
                    return Ok(Value::Null(value.into()));
                }
                let id = ffi::value_get_internal_id(id);

                let id = InternalID {
                    offset: id[0],
                    table_id: id[1],
                };
                let label = ffi::node_value_get_label_name(value);
                let mut node_val = NodeVal::new(id, label);
                for i in 0..ffi::node_value_get_num_properties(value) {
                    node_val.add_property(
                        ffi::node_value_get_property_name(value, i),
                        TryInto::<Value>::try_into(ffi::node_value_get_property_value(value, i))?,
                    );
                }
                Ok(Value::Node(node_val))
            }
            LogicalTypeID::REL => {
                let src_node = ffi::rel_value_get_src_id(value);
                if ffi::value_is_null(src_node) {
                    return Ok(Value::Null(value.into()));
                }
                let src_node = ffi::value_get_internal_id(src_node);

                let dst_node = ffi::rel_value_get_dst_id(value);
                let src_node = InternalID {
                    offset: src_node[0],
                    table_id: src_node[1],
                };
                let dst_node = InternalID {
                    offset: dst_node[0],
                    table_id: dst_node[1],
                };
                let label = ffi::rel_value_get_label_name(value);
                let mut rel_val = RelVal::new(src_node, dst_node, label);
                for i in 0..ffi::rel_value_get_num_properties(value) {
                    rel_val.add_property(
                        ffi::rel_value_get_property_name(value, i),
                        ffi::rel_value_get_property_value(value, i).try_into()?,
                    );
                }
                Ok(Value::Rel(rel_val))
            }
            LogicalTypeID::INTERNAL_ID => {
                let internal_id = ffi::value_get_internal_id(value);
                Ok(Value::InternalID(InternalID {
                    offset: internal_id[0],
                    table_id: internal_id[1],
                }))
            }
            LogicalTypeID::RECURSIVE_REL => {
                let nodes: Value = ffi::recursive_rel_get_nodes(value).try_into()?;
                let rels: Value = ffi::recursive_rel_get_rels(value).try_into()?;
                let nodes = if let Value::List(LogicalType::Node, nodes) = nodes {
                    nodes.into_iter().map(|x| {
                        if let Value::Node(x) = x {
                            x
                        } else {
                            unreachable!()
                        }
                    })
                } else {
                    panic!("Unexpected value in RecursiveRel's rels: {rels}")
                };
                let rels = if let Value::List(LogicalType::Rel, rels) = rels {
                    rels.into_iter().map(|x| {
                        if let Value::Rel(x) = x {
                            x
                        } else {
                            unreachable!()
                        }
                    })
                } else {
                    panic!("Unexpected value in RecursiveRel's rels: {rels}")
                };

                Ok(Value::RecursiveRel {
                    nodes: nodes.collect(),
                    rels: rels.collect(),
                })
            }
            LogicalTypeID::UNION => {
                let LogicalType::Union { types } = ffi::value_get_data_type(value).into() else {
                    unreachable!()
                };
                debug_assert!(ffi::value_get_children_size(value) == 1);
                let value: Value = ffi::value_get_child(value, 0).try_into()?;
                Ok(Value::Union {
                    types,
                    value: Box::new(value),
                })
            }
            LogicalTypeID::DECIMAL => {
                let logical_type: LogicalType = ffi::value_get_data_type(value).into();
                if let LogicalType::Decimal {
                    scale,
                    precision: _,
                } = logical_type
                {
                    let decimal_value: i128 = match ffi::value_get_physical_type(value) {
                        PhysicalTypeID::INT128 => get_i128(value),
                        PhysicalTypeID::INT64 => i128::from(ffi::value_get_i64(value)),
                        PhysicalTypeID::INT32 => i128::from(ffi::value_get_i32(value)),
                        PhysicalTypeID::INT16 => i128::from(ffi::value_get_i16(value)),
                        PhysicalTypeID::INT8 => i128::from(ffi::value_get_i8(value)),
                        _ => unreachable!(),
                    };
                    let decimal =
                        rust_decimal::Decimal::try_from_i128_with_scale(decimal_value, scale)
                            .map_err(|err| ConversionError::Decimal(decimal_value, scale, err))?;
                    Ok(Value::Decimal(decimal))
                } else {
                    unreachable!()
                }
            }
            // TODO(bmwinger): Better error message for types which are unsupported
            x => panic!("Unsupported type {x:?}"),
        }
    }
}

impl TryInto<cxx::UniquePtr<ffi::Value>> for Value {
    // Errors should occur if:
    // - types are heterogeneous in lists
    type Error = crate::error::Error;

    fn try_into(self) -> Result<cxx::UniquePtr<ffi::Value>, Self::Error> {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        fn get_high_low(value: i128) -> (i64, u64) {
            ((value >> 64) as i64, value as u64)
        }

        fn date_to_lbug_date_t(value: time::Date) -> i32 {
            // Convert to days since 1970-01-01
            (value - time::Date::from_ordinal_date(1970, 1).unwrap())
                .whole_days()
                .to_i32()
                .expect("lbug_date i64->i32 overflow")
        }

        fn datetime_to_timestamp_t(value: time::OffsetDateTime) -> i64 {
            // Convert to microseconds since 1970-01-01. Floor division (not
            // truncation toward zero) so that pre-epoch instants with a
            // sub-microsecond fraction round to the previous microsecond,
            // matching the engine's own cast behaviour (ladybug-rust#48).
            (value.unix_timestamp_nanos().div_euclid(1000))
                .to_i64()
                .expect("timestamp i128->i64 overflow")
        }

        fn get_interval_t(value: time::Duration) -> (i32, i32, i64) {
            use time::Duration;
            let mut interval = value;
            let months = interval.whole_days() / 30;
            interval -= Duration::days(months * 30);
            let days = interval.whole_days();
            interval -= Duration::days(days);
            let months = months.to_i32().expect("months i64->i32 overflow");
            let days = days.to_i32().expect("days i64->i32 overflow");
            let micros = interval
                .whole_microseconds()
                .to_i64()
                .expect("micros i128->i64 overflow");
            (months, days, micros)
        }

        match self {
            Value::Null(typ) => Ok(ffi::create_value_null((&typ).into())),
            Value::Bool(value) => Ok(ffi::create_value_bool(value)),
            Value::Int8(value) => Ok(ffi::create_value_i8(value)),
            Value::Int16(value) => Ok(ffi::create_value_i16(value)),
            Value::Int32(value) => Ok(ffi::create_value_i32(value)),
            Value::Int64(value) => Ok(ffi::create_value_i64(value)),
            Value::UInt8(value) => Ok(ffi::create_value_u8(value)),
            Value::UInt16(value) => Ok(ffi::create_value_u16(value)),
            Value::UInt32(value) => Ok(ffi::create_value_u32(value)),
            Value::UInt64(value) => Ok(ffi::create_value_u64(value)),
            Value::Int128(value) => {
                let (high, low) = get_high_low(value);
                Ok(ffi::create_value_int128_t(high, low))
            }
            #[allow(clippy::cast_possible_wrap)]
            Value::UUID(value) => {
                // values are stored as i128 and the first bit flipped so that they order as if
                // they are u128
                let value = value.as_u128() as i128;
                let (high, low) = get_high_low(value ^ (1 << 127));
                Ok(ffi::create_value_uuid_t(high, low))
            }
            Value::Float(value) => Ok(ffi::create_value_float(value)),
            Value::Double(value) => Ok(ffi::create_value_double(value)),
            Value::String(value) => Ok(ffi::create_value_string(
                ffi::LogicalTypeID::STRING,
                value.as_bytes(),
            )),
            Value::Json(value) => Ok(ffi::create_value_string(
                ffi::LogicalTypeID::JSON,
                &serde_json::to_vec(&value)?,
            )),
            Value::Blob(value) => Ok(ffi::create_value_string(ffi::LogicalTypeID::BLOB, &value)),
            Value::Timestamp(value) => {
                Ok(ffi::create_value_timestamp(datetime_to_timestamp_t(value)))
            }
            Value::TimestampTz(value) => Ok(ffi::create_value_timestamp_tz(
                // Convert to microseconds since 1970-01-01, flooring like
                // `datetime_to_timestamp_t` (ladybug-rust#48).
                (value.unix_timestamp_nanos().div_euclid(1000))
                    .to_i64()
                    .expect("TimestampTz i128->i64 overflow"),
            )),
            Value::TimestampNs(value) => Ok(ffi::create_value_timestamp_ns(
                value
                    .unix_timestamp_nanos()
                    .to_i64()
                    .expect("TimestampMs i128->i64 overflow"),
            )),
            Value::TimestampMs(value) => Ok(ffi::create_value_timestamp_ms(
                (value.unix_timestamp_nanos() / 1_000_000)
                    .to_i64()
                    .expect("TimestampMs i128->i64 overflow"),
            )),
            Value::TimestampSec(value) => Ok(ffi::create_value_timestamp_sec(
                (value.unix_timestamp_nanos() / 1_000_000_000)
                    .to_i64()
                    .expect("TimestampSec i128->i64 overflow"),
            )),
            Value::Date(value) => Ok(ffi::create_value_date(date_to_lbug_date_t(value))),
            Value::Interval(value) => {
                let (months, days, micros) = get_interval_t(value);
                Ok(ffi::create_value_interval(months, days, micros))
            }
            Value::List(typ, value) => {
                let mut builder = ffi::create_list();
                for elem in value {
                    ffi::value_list_insert(builder.pin_mut(), elem.try_into()?);
                }
                Ok(ffi::get_list_value(
                    (&LogicalType::List {
                        child_type: Box::new(typ),
                    })
                        .into(),
                    builder,
                ))
            }
            Value::Map((key_type, value_type), values) => {
                let mut builder = ffi::create_list();
                let list_type = LogicalType::Struct {
                    fields: vec![
                        ("KEY".to_string(), key_type.clone()),
                        ("VALUE".to_string(), value_type.clone()),
                    ],
                };
                for (key, value) in values {
                    let mut pair = ffi::create_list();
                    ffi::value_list_insert(pair.pin_mut(), key.try_into()?);
                    ffi::value_list_insert(pair.pin_mut(), value.try_into()?);
                    let pair_value = ffi::get_list_value((&list_type).into(), pair);
                    ffi::value_list_insert(builder.pin_mut(), pair_value);
                }
                Ok(ffi::get_list_value(
                    (&LogicalType::Map {
                        key_type: Box::new(key_type),
                        value_type: Box::new(value_type),
                    })
                        .into(),
                    builder,
                ))
            }
            Value::Array(typ, value) => {
                let mut builder = ffi::create_list();
                let len = value.len();
                for elem in value {
                    ffi::value_list_insert(builder.pin_mut(), elem.try_into()?);
                }
                Ok(ffi::get_list_value(
                    (&LogicalType::Array {
                        child_type: Box::new(typ),
                        num_elements: len as u64,
                    })
                        .into(),
                    builder,
                ))
            }
            Value::Struct(value) => {
                let typ: LogicalType = LogicalType::Struct {
                    fields: value
                        .iter()
                        .map(|(name, value)| (name.clone(), Into::<LogicalType>::into(value)))
                        .collect(),
                };

                let mut builder = ffi::create_list();
                for (_, elem) in value {
                    ffi::value_list_insert(builder.pin_mut(), elem.try_into()?);
                }

                Ok(ffi::get_list_value((&typ).into(), builder))
            }
            Value::InternalID(value) => {
                Ok(ffi::create_value_internal_id(value.offset, value.table_id))
            }
            Value::Node(_) => Err(crate::Error::ReadOnlyType(LogicalType::Node)),
            Value::Rel(_) => Err(crate::Error::ReadOnlyType(LogicalType::Rel)),
            Value::RecursiveRel { .. } => {
                Err(crate::Error::ReadOnlyType(LogicalType::RecursiveRel))
            }
            Value::Union { types, value } => {
                let mut builder = ffi::create_list();
                ffi::value_list_insert(builder.pin_mut(), (*value).try_into()?);

                Ok(ffi::get_list_value(
                    (&LogicalType::Union { types }).into(),
                    builder,
                ))
            }
            Value::Decimal(decimal_value) => {
                let (high, low) = get_high_low(decimal_value.mantissa());
                if let LogicalType::Decimal { scale, precision } = (&self).into() {
                    Ok(ffi::create_value_decimal(high, low, scale, precision))
                } else {
                    unreachable!()
                }
            }
        }
    }
}

impl From<i8> for Value {
    fn from(item: i8) -> Self {
        Value::Int8(item)
    }
}

impl From<i16> for Value {
    fn from(item: i16) -> Self {
        Value::Int16(item)
    }
}

impl From<i32> for Value {
    fn from(item: i32) -> Self {
        Value::Int32(item)
    }
}

impl From<i64> for Value {
    fn from(item: i64) -> Self {
        Value::Int64(item)
    }
}

impl From<u8> for Value {
    fn from(item: u8) -> Self {
        Value::UInt8(item)
    }
}

impl From<u16> for Value {
    fn from(item: u16) -> Self {
        Value::UInt16(item)
    }
}

impl From<u32> for Value {
    fn from(item: u32) -> Self {
        Value::UInt32(item)
    }
}

impl From<u64> for Value {
    fn from(item: u64) -> Self {
        Value::UInt64(item)
    }
}

impl From<i128> for Value {
    fn from(item: i128) -> Self {
        Value::Int128(item)
    }
}

impl From<f32> for Value {
    fn from(item: f32) -> Self {
        Value::Float(item)
    }
}

impl From<f64> for Value {
    fn from(item: f64) -> Self {
        Value::Double(item)
    }
}

impl From<String> for Value {
    fn from(item: String) -> Self {
        Value::String(item)
    }
}

impl From<serde_json::Value> for Value {
    fn from(item: serde_json::Value) -> Self {
        Value::Json(item)
    }
}

impl From<&str> for Value {
    fn from(item: &str) -> Self {
        Value::String(item.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::ConversionError;
    use crate::database::SYSTEM_CONFIG_FOR_TESTS;
    use crate::ffi::ffi;
    use crate::{Connection, Database, InternalID, LogicalType, NodeVal, RelVal, Value};
    use anyhow::Result;
    use rust_decimal_macros::dec;
    use std::collections::HashSet;
    use std::convert::TryInto;
    use std::iter::FromIterator;
    use time::macros::{date, datetime};
    use uuid::uuid;

    macro_rules! type_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            /// Tests that the values are correctly converted into `lbug::common::Value` and back
            fn $name() -> Result<()> {
                let rust_type: LogicalType = $value;
                let typ: cxx::UniquePtr<ffi::LogicalType> = (&rust_type).try_into()?;
                let new_rust_type: LogicalType = typ.as_ref().unwrap().try_into()?;
                assert_eq!(new_rust_type, rust_type);
                Ok(())
            }
        )*
        }
    }

    macro_rules! value_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            /// Tests that the values are correctly converted into `lbug::common::Value` and back
            fn $name() -> Result<()> {
                let rust_value: Value = $value;
                let value: cxx::UniquePtr<ffi::Value> = rust_value.clone().try_into()?;
                let new_rust_value: Value = value.as_ref().unwrap().try_into()?;
                assert_eq!(new_rust_value, rust_value);
                Ok(())
            }
        )*
        }
    }

    macro_rules! display_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            /// Tests that the values display the same via the rust API as via the C++ API
            fn $name() -> Result<()> {
                let rust_value: Value = $value;
                let value: cxx::UniquePtr<ffi::Value> = rust_value.clone().try_into()?;
                assert_eq!(ffi::value_to_string(value.as_ref().unwrap()), format!("{rust_value}"));
                Ok(())
            }
        )*
        }
    }

    macro_rules! database_tests {
        ($($name:ident: $value:expr, $decl:expr,)*) => {
        $(
            #[test]
            /// Tests that passing the values through the database returns what we put in
            fn $name() -> Result<()> {
                let temp_dir = tempfile::tempdir()?;
                let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
                let conn = Connection::new(&db)?;
                conn.query(&format!(
                    "CREATE NODE TABLE Person(name STRING, item {}, PRIMARY KEY(name));",
                    $decl,
                ))?;

                let mut add_person =
                    conn.prepare("CREATE (:Person {name: $name, item: $item});")?;
                conn.execute(
                    &mut add_person,
                    vec![("name", "Bob".into()), ("item", $value)],
                )?;
                let result = conn
                    .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN a.item;")?
                    .next()
                    .unwrap()?;
                // TODO: Test equivalence to value constructed inside a a query
                assert_eq!(result[0], $value);
                temp_dir.close()?;
                Ok(())
            }
        )*
        }
    }

    type_tests! {
        convert_list_type: LogicalType::List { child_type: Box::new(LogicalType::String) },
        convert_array_type: LogicalType::Array { child_type: Box::new(LogicalType::Int64), num_elements: 3 },
        convert_int8_type: LogicalType::Int8,
        convert_int16_type: LogicalType::Int16,
        convert_int32_type: LogicalType::Int32,
        convert_int64_type: LogicalType::Int64,
        convert_uint8_type: LogicalType::UInt8,
        convert_uint16_type: LogicalType::UInt16,
        convert_uint32_type: LogicalType::UInt32,
        convert_uint64_type: LogicalType::UInt64,
        convert_int128_type: LogicalType::Int128,
        convert_uuid_type: LogicalType::UUID,
        convert_float_type: LogicalType::Float,
        convert_double_type: LogicalType::Double,
        convert_timestamp_type: LogicalType::Timestamp,
        convert_timestamp_tz_type: LogicalType::TimestampTz,
        convert_timestamp_ns_type: LogicalType::TimestampNs,
        convert_timestamp_ms_type: LogicalType::TimestampMs,
        convert_timestamp_sec_type: LogicalType::TimestampSec,
        convert_date_type: LogicalType::Date,
        convert_interval_type: LogicalType::Interval,
        convert_string_type: LogicalType::String,
        convert_blob_type: LogicalType::Blob,
        convert_bool_type: LogicalType::Bool,
        convert_struct_type: LogicalType::Struct { fields: vec![("NAME".to_string(), LogicalType::String)]},
        convert_node_type: LogicalType::Node,
        convert_internal_id_type: LogicalType::InternalID,
        convert_rel_type: LogicalType::Rel,
        convert_recursive_rel_type: LogicalType::RecursiveRel,
        convert_map_type: LogicalType::Map { key_type: Box::new(LogicalType::Interval), value_type: Box::new(LogicalType::Rel) },
        convert_union_type: LogicalType::Union { types: vec![("Num".to_string(), LogicalType::Int8), ("duration".to_string(), LogicalType::Interval), ("string".to_string(), LogicalType::String)] },
        convert_decimal_type: LogicalType::Decimal { scale: 3, precision: 9 },
    }

    value_tests! {
        convert_list: Value::List(LogicalType::String, vec!["Alice".into(), "Bob".into()]),
        convert_list_empty: Value::List(LogicalType::String, vec![]),
        convert_array: Value::Array(LogicalType::String, vec!["Alice".into(), "Bob".into()]),
        convert_int8: Value::Int8(0),
        convert_int16: Value::Int16(1),
        convert_int32: Value::Int32(2),
        convert_int64: Value::Int64(3),
        convert_uint8: Value::UInt8(0),
        convert_uint16: Value::UInt16(1),
        convert_uint32: Value::UInt32(2),
        convert_uint64: Value::UInt64(3),
        convert_int128: Value::Int128(1),
        convert_int128_negative: Value::Int128(-1),
        convert_int128_large: Value::Int128(184467440737095516158),
        convert_int128_large_negative: Value::Int128(-184467440737095516158),
        convert_uuid: Value::UUID(uuid!("00000000-0000-0000-0000-ffff00000000")),
        convert_uuid2: Value::UUID(uuid!("a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11")),
        convert_float: Value::Float(4.),
        convert_double: Value::Double(5.),
        convert_timestamp: Value::Timestamp(datetime!(2023-06-13 11:25:30 UTC)),
        convert_timestamp_tz: Value::TimestampTz(datetime!(2023-06-13 11:25:30 UTC)),
        convert_timestamp_ns: Value::TimestampNs(datetime!(2023-06-13 11:25:30.12345 UTC)),
        convert_timestamp_ms: Value::TimestampMs(datetime!(2023-06-13 11:25:30.123 UTC)),
        convert_timestamp_sec: Value::TimestampSec(datetime!(2023-06-13 11:25:30 UTC)),
        convert_date: Value::Date(date!(2023-06-13)),
        convert_interval: Value::Interval(time::Duration::weeks(10)),
        convert_string: Value::String("Hello World".to_string()),
        convert_blob: Value::Blob("Hello World".into()),
        convert_bool: Value::Bool(false),
        convert_null: Value::Null(LogicalType::List {
            child_type: Box::new(LogicalType::Array { child_type: Box::new(LogicalType::Int16), num_elements: 3 })
        }),
        convert_struct: Value::Struct(vec![("NAME".to_string(), "Alice".into()), ("AGE".to_string(), 25.into())]),
        convert_internal_id: Value::InternalID(InternalID { table_id: 0, offset: 0 }),
        convert_map: Value::Map((LogicalType::String, LogicalType::Int64), vec![(Value::String("key".to_string()), Value::Int64(24))]),
        convert_union: Value::Union {
            types: vec![("Num".to_string(), LogicalType::Int8), ("duration".to_string(), LogicalType::Interval)],
            value: Box::new(Value::Int8(-127))
        },
        convert_decimal16: Value::Decimal(dec!(12.34)),
        convert_decimal32: Value::Decimal(dec!(12.3456789)),
        convert_decimal64: Value::Decimal(dec!(12.34567890)),
        convert_decimal128: Value::Decimal(dec!(12.34567890)),
        convert_decimal_negative: Value::Decimal(dec!(-12345678.123456)),
        convert_decimal_negative_large: Value::Decimal(dec!(-7922816251426433759.3543950335)),
        convert_decimal_negative_fraction: Value::Decimal(dec!(-0.9999999999)),
        convert_decimal_tiny: Value::Decimal(dec!(0.0000000000000000000000000001)),
    }

    display_tests! {
        display_list: Value::List(LogicalType::String, vec!["Alice".into(), "Bob".into()]),
        display_list_empty: Value::List(LogicalType::String, vec![]),
        display_array: Value::Array(LogicalType::String, vec!["Alice".into(), "Bob".into()]),
        display_int8: Value::Int8(0),
        display_int16: Value::Int16(1),
        display_int32: Value::Int32(2),
        display_int64: Value::Int64(3),
        display_uint8: Value::UInt8(0),
        display_uint16: Value::UInt16(1),
        display_uint32: Value::UInt32(2),
        display_uint64: Value::UInt64(3),
        // Float, double, interval and timestamp have display differences which we probably don't want to
        // reconcile
        display_date: Value::Date(date!(2023-06-13)),
        // blob may contain non-utf8 data, so we display it as an array rather than a string
        // The C++ API escapes data in the blob as hex
        display_string: Value::String("Hello World".to_string()),
        display_bool: Value::Bool(false),
        display_null: Value::Null(LogicalType::List {
            child_type: Box::new(LogicalType::Array { child_type: Box::new(LogicalType::Int16), num_elements: 3 })
        }),
        display_struct: Value::Struct(vec![("NAME".to_string(), "Alice".into()), ("AGE".to_string(), 25.into())]),
        display_internal_id: Value::InternalID(InternalID { table_id: 0, offset: 0 }),
        // Node and Rel Cannot be easily created on the C++ side
        display_map: Value::Map((LogicalType::String, LogicalType::Int64), vec![(Value::String("key".to_string()), Value::Int64(24))]),
        display_union: Value::Union {
            types: vec![("Num".to_string(), LogicalType::Int8), ("duration".to_string(), LogicalType::Interval)],
            value: Box::new(Value::Int8(-127))
        },
        display_uuid: Value::UUID(uuid!("00000000-0000-0000-0000-ffff00000000")),
        display_uuid2: Value::UUID(uuid!("a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11")),
        display_uuid3: Value::UUID(uuid!("a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d8")),
        display_decimal16: Value::Decimal(dec!(12.34)),
        display_decimal32: Value::Decimal(dec!(12.3456789)),
        display_decimal64: Value::Decimal(dec!(12.34567890)),
        display_decimal128: Value::Decimal(dec!(12.34567890)),
        display_decimal_negative: Value::Decimal(dec!(-12345678.123456)),
        display_decimal_negative_large: Value::Decimal(dec!(-7922816251426433759.3543950335)),
    }

    #[test]
    /// Regression test for ladybug-rust#48 (C5): pre-epoch `Timestamp` and
    /// `TimestampTz` values with a sub-microsecond fraction must floor to the
    /// previous microsecond when converted to the engine's microsecond
    /// representation, matching the engine's own cast behaviour, instead of
    /// truncating toward zero (which moved -1ns to the epoch).
    fn timestamp_pre_epoch_sub_microsecond_floors() -> Result<()> {
        let epoch = time::OffsetDateTime::UNIX_EPOCH;
        let cases = [
            (
                time::Duration::nanoseconds(-1),
                time::Duration::microseconds(-1),
            ),
            (
                time::Duration::nanoseconds(-1000),
                time::Duration::microseconds(-1),
            ),
            (
                time::Duration::nanoseconds(-1001),
                time::Duration::microseconds(-2),
            ),
            (
                time::Duration::nanoseconds(1999),
                time::Duration::microseconds(1),
            ),
            (
                time::Duration::nanoseconds(0),
                time::Duration::microseconds(0),
            ),
        ];
        for (offset, expected_offset) in cases {
            let value = Value::Timestamp(epoch + offset);
            let ffi_value: cxx::UniquePtr<ffi::Value> = value.try_into()?;
            let read_back: Value = ffi_value.as_ref().unwrap().try_into()?;
            assert_eq!(read_back, Value::Timestamp(epoch + expected_offset));

            let value_tz = Value::TimestampTz(epoch + offset);
            let ffi_value_tz: cxx::UniquePtr<ffi::Value> = value_tz.try_into()?;
            let read_back_tz: Value = ffi_value_tz.as_ref().unwrap().try_into()?;
            assert_eq!(read_back_tz, Value::TimestampTz(epoch + expected_offset));
        }
        Ok(())
    }

    #[test]
    /// Regression test for ladybug-rust#48 (C5): a bound pre-epoch timestamp
    /// must be stored as the floored microsecond value, so the stored value
    /// matches what the engine itself produces for the same instant.
    fn timestamp_pre_epoch_sub_microsecond_db_round_trip() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query("CREATE NODE TABLE T(id INT64, ts TIMESTAMP, PRIMARY KEY(id));")?;
        let mut stmt = conn.prepare("CREATE (:T {id: $id, ts: $ts});")?;
        let minus_one_ns = time::OffsetDateTime::UNIX_EPOCH - time::Duration::nanoseconds(1);
        conn.execute(
            &mut stmt,
            vec![("id", 1i64.into()), ("ts", Value::Timestamp(minus_one_ns))],
        )?;
        let result = conn.query("MATCH (a:T) RETURN a.ts;")?.next().unwrap()?;
        assert_eq!(
            result[0],
            Value::Timestamp(time::OffsetDateTime::UNIX_EPOCH - time::Duration::microseconds(1))
        );
        temp_dir.close()?;
        Ok(())
    }

    database_tests! {
        // Passing these values as arguments is not yet implemented in lbug:
        // db_union: Value::Union {
        //    types: vec![("Num".to_string(), LogicalType::Int8), ("duration".to_string(), LogicalType::Interval)],
        //    value: Box::new(Value::Int8(-127))
        // }, "UNION(Num INT8, duration INTERVAL)",
        db_list_string: Value::List(LogicalType::String, vec!["Alice".into(), "Bob".into()]), "STRING[]",
        db_list_int: Value::List(LogicalType::Int64, vec![0i64.into(), 1i64.into(), 2i64.into()]), "INT64[]",
        db_map: Value::Map((LogicalType::String, LogicalType::Int64), vec![(Value::String("key".to_string()), Value::Int64(24))]), "MAP(STRING,INT64)",
        db_array: Value::Array(LogicalType::Int64, vec![1i64.into(), 2i64.into(), 3i64.into()]), "INT64[3]",
        db_struct:
           Value::Struct(vec![("item".to_string(), "Knife".into()), ("count".to_string(), 1.into())]),
           "STRUCT(item STRING, count INT32)",
        db_null_string: Value::Null(LogicalType::String), "STRING",
        db_null_int: Value::Null(LogicalType::Int64), "INT64",
        db_null_list: Value::Null(LogicalType::List {
           child_type: Box::new(LogicalType::Array { child_type: Box::new(LogicalType::Int16), num_elements: 3 })
        }), "INT16[3][]",
        db_int8: Value::Int8(0), "INT8",
        db_int16: Value::Int16(1), "INT16",
        db_int32: Value::Int32(2), "INT32",
        db_int64: Value::Int64(3), "INT64",
        db_uint8: Value::UInt8(0), "UINT8",
        db_uint16: Value::UInt16(1), "UINT16",
        db_uint32: Value::UInt32(2), "UINT32",
        db_uint64: Value::UInt64(3), "UINT64",
        db_int128: Value::Int128(4), "INT128",
        db_float: Value::Float(4.), "FLOAT",
        db_double: Value::Double(5.), "DOUBLE",
        db_timestamp: Value::Timestamp(datetime!(2023-06-13 11:25:30 UTC)), "TIMESTAMP",
        db_timestamp_tz: Value::TimestampTz(datetime!(2023-06-13 11:25:30.12345 UTC)), "TIMESTAMP_TZ",
        db_timestamp_ns: Value::TimestampNs(datetime!(2023-06-13 11:25:30.12345 UTC)), "TIMESTAMP_NS",
        db_timestamp_ms: Value::TimestampMs(datetime!(2023-06-13 11:25:30.123 UTC)), "TIMESTAMP_MS",
        db_timestamp_sec: Value::TimestampSec(datetime!(2023-06-13 11:25:30 UTC)), "TIMESTAMP_SEC",
        db_date: Value::Date(date!(2023-06-13)), "DATE",
        db_interval: Value::Interval(time::Duration::weeks(200)), "INTERVAL",
        db_string: Value::String("Hello World".to_string()), "STRING",
        db_blob: Value::Blob("Hello World".into()), "BLOB",
        db_bool: Value::Bool(true), "BOOLEAN",
        db_uuid: Value::UUID(uuid!("00000000-0000-0000-0000-ffff00000000")), "UUID",
        db_uuid2: Value::UUID(uuid!("8f914bce-df4e-4244-9cd4-ea96bf0c58d4")), "UUID",
        db_decimal16: Value::Decimal(dec!(12.34)), "DECIMAL(4, 2)",
        db_decimal32: Value::Decimal(dec!(12.3456789)), "DECIMAL(9, 7)",
        db_decimal64: Value::Decimal(dec!(12.34567890)), "DECIMAL(18, 8)",
        db_decimal128: Value::Decimal(dec!(12.34567890)), "DECIMAL(38, 8)",
        db_decimal_negative: Value::Decimal(dec!(-12345678.123456)), "DECIMAL(14, 6)",
        db_decimal_negative_large: Value::Decimal(dec!(-7922816251426433759.3543950335)), "DECIMAL(38, 10)",
        db_decimal_negative_mid: Value::Decimal(dec!(-9999999.999950001)), "DECIMAL(38, 9)",
        db_decimal_negative_fraction: Value::Decimal(dec!(-0.9999999999)), "DECIMAL(38, 10)",
        db_decimal_negative_boundary: Value::Decimal(dec!(-0.32769)), "DECIMAL(38, 5)",
        db_decimal_negative_boundary_control: Value::Decimal(dec!(-0.32767)), "DECIMAL(38, 5)",
        db_decimal_negative_one: Value::Decimal(dec!(-1)), "DECIMAL(38, 10)",
        db_decimal_negative_tiny: Value::Decimal(dec!(-0.0000000001)), "DECIMAL(38, 10)",
    }

    #[test]
    /// Tests that the type derived from a negative decimal counts the digits
    /// of the mantissa's magnitude, not 1 (the pre-fix `checked_ilog10`
    /// returned `None` for every non-positive mantissa)
    fn decimal_derived_type_negative() {
        assert_eq!(
            LogicalType::from(&Value::Decimal(dec!(-12345678.123456))),
            LogicalType::Decimal {
                precision: 14,
                scale: 6
            }
        );
        assert_eq!(
            LogicalType::from(&Value::Decimal(dec!(-0.9999999999))),
            LogicalType::Decimal {
                precision: 10,
                scale: 10
            }
        );
        assert_eq!(
            LogicalType::from(&Value::Decimal(dec!(-1))),
            LogicalType::Decimal {
                precision: 1,
                scale: 0
            }
        );
    }

    #[test]
    /// Tests that the type derived from the largest-magnitude negative
    /// decimal uses all 29 digits of its mantissa
    fn decimal_derived_type_negative_max() {
        assert_eq!(
            LogicalType::from(&Value::Decimal(dec!(-7922816251426433759.3543950335))),
            LogicalType::Decimal {
                precision: 29,
                scale: 10
            }
        );
    }

    #[test]
    /// Tests that a value whose scale exceeds its mantissa's digit count
    /// derives a valid type with precision raised to the scale (the pre-fix
    /// derivation produced the invalid DECIMAL(1, 28))
    fn decimal_derived_type_tiny_scale28() {
        assert_eq!(
            LogicalType::from(&Value::Decimal(dec!(0.0000000000000000000000000001))),
            LogicalType::Decimal {
                precision: 28,
                scale: 28
            }
        );
    }

    #[test]
    /// Tests that zero derives DECIMAL(1, 0), as it did before the fix
    fn decimal_derived_type_zero() {
        assert_eq!(
            LogicalType::from(&Value::Decimal(dec!(0))),
            LogicalType::Decimal {
                precision: 1,
                scale: 0
            }
        );
    }

    #[test]
    /// Tests that an ordinary positive decimal derives the same type as it
    /// did before the fix
    fn decimal_derived_type_positive_unchanged() {
        assert_eq!(
            LogicalType::from(&Value::Decimal(dec!(12.34))),
            LogicalType::Decimal {
                precision: 4,
                scale: 2
            }
        );
    }

    #[test]
    /// Tests that a negative decimal bound into the filed DECIMAL(12, 4)
    /// column is stored as the correct rescale of the bound value (the
    /// filed corruption read back -0.0209)
    fn db_decimal_filed_column_negative() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query(
            "CREATE NODE TABLE Person(name STRING, item DECIMAL(12, 4), PRIMARY KEY(name));",
        )?;
        let mut add_person = conn.prepare("CREATE (:Person {name: $name, item: $item});")?;
        conn.execute(
            &mut add_person,
            vec![
                ("name", "Bob".into()),
                ("item", Value::Decimal(dec!(-12345678.123456))),
            ],
        )?;
        let result = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN a.item;")?
            .next()
            .unwrap()?;
        assert_eq!(result[0], Value::Decimal(dec!(-12345678.1235)));
        let result = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN CAST(a.item AS STRING);")?
            .next()
            .unwrap()?;
        assert_eq!(result[0], Value::String("-12345678.1235".to_string()));
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Tests that the largest-magnitude negative decimal bound into the
    /// filed DECIMAL(38, 10) column round-trips exactly (the filed
    /// corruption read back 0.0000000001)
    fn db_decimal_filed_column_negative_large() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query(
            "CREATE NODE TABLE Person(name STRING, item DECIMAL(38, 10), PRIMARY KEY(name));",
        )?;
        let mut add_person = conn.prepare("CREATE (:Person {name: $name, item: $item});")?;
        conn.execute(
            &mut add_person,
            vec![
                ("name", "Bob".into()),
                (
                    "item",
                    Value::Decimal(dec!(-7922816251426433759.3543950335)),
                ),
            ],
        )?;
        let result = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN a.item;")?
            .next()
            .unwrap()?;
        assert_eq!(
            result[0],
            Value::Decimal(dec!(-7922816251426433759.3543950335))
        );
        let result = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN CAST(a.item AS STRING);")?
            .next()
            .unwrap()?;
        assert_eq!(
            result[0],
            Value::String("-7922816251426433759.3543950335".to_string())
        );
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Tests that a negative decimal bound into the filed DECIMAL(12, 4)
    /// column is stored sign-symmetrically with its positive counterpart
    /// (the filed corruption read back 0.0000)
    fn db_decimal_filed_column_negative_mid() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query(
            "CREATE NODE TABLE Person(name STRING, item DECIMAL(12, 4), PRIMARY KEY(name));",
        )?;
        let mut add_person = conn.prepare("CREATE (:Person {name: $name, item: $item});")?;
        conn.execute(
            &mut add_person,
            vec![
                ("name", "Bob".into()),
                ("item", Value::Decimal(dec!(-9999999.999950001))),
            ],
        )?;
        conn.execute(
            &mut add_person,
            vec![
                ("name", "Alice".into()),
                ("item", Value::Decimal(dec!(9999999.999950001))),
            ],
        )?;
        let negative = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN a.item;")?
            .next()
            .unwrap()?;
        let positive = conn
            .query("MATCH (a:Person) WHERE a.name = \"Alice\" RETURN a.item;")?
            .next()
            .unwrap()?;
        let (Value::Decimal(negative), Value::Decimal(positive)) = (&negative[0], &positive[0])
        else {
            panic!("expected decimal values");
        };
        assert_eq!(*negative, -*positive);
        assert_ne!(*negative, dec!(0.0000));
        let as_string = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN CAST(a.item AS STRING);")?
            .next()
            .unwrap()?;
        assert_eq!(as_string[0], Value::String(format!("{negative}")));
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Tests that a tiny scale-28 decimal binds with its derived type DECIMAL(28, 28)
    /// across FFI and can be queried back cleanly without error.
    fn db_decimal_tiny_scale28_binds_decimal_28_28() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        let tiny = Value::Decimal(dec!(0.0000000000000000000000000001));
        assert_eq!(
            LogicalType::from(&tiny),
            LogicalType::Decimal {
                precision: 28,
                scale: 28
            }
        );
        let mut prep = conn.prepare("RETURN $val AS res;")?;
        let mut result = conn.execute(&mut prep, vec![("val", tiny.clone())])?;
        let row = result.next().expect("row")?;
        assert_eq!(row[0], tiny);
        Ok(())
    }

    #[test]
    /// Tests that extreme downscales of tiny scale-28 decimals bound into
    /// the filed DECIMAL(12, 4) column and the DECIMAL(18, 4) column
    /// deterministically store 0.0000. Pins determinism under downscale.
    /// (Note: pre-fix nondeterministic garbage was reported in author isolation
    /// testing under derived DECIMAL(1, 28), but was not independently reproduced
    /// on macOS arm64 where base also stored 0.0000).
    fn db_decimal_downscale_tiny_scale28_deterministic() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query(
            "CREATE NODE TABLE Filed12(name STRING, item DECIMAL(12, 4), PRIMARY KEY(name));",
        )?;
        conn.query(
            "CREATE NODE TABLE Filed18(name STRING, item DECIMAL(18, 4), PRIMARY KEY(name));",
        )?;
        conn.query(
            "CREATE NODE TABLE Control38(name STRING, item DECIMAL(38, 28), PRIMARY KEY(name));",
        )?;

        // 25 rows of 1e-28 per filed target, plus 5 rows each of 2e-28 and
        // -1e-28 in the (12, 4) table: 60 filed-class inserts in total.
        let tiny = Value::Decimal(dec!(0.0000000000000000000000000001));
        let mut filed12_values = vec![tiny.clone(); 25];
        filed12_values.extend(vec![
            Value::Decimal(dec!(0.0000000000000000000000000002));
            5
        ]);
        filed12_values.extend(vec![
            Value::Decimal(dec!(-0.0000000000000000000000000001));
            5
        ]);
        let filed18_values = vec![tiny.clone(); 25];

        let mut add_filed12 = conn.prepare("CREATE (:Filed12 {name: $name, item: $item});")?;
        for (i, value) in filed12_values.iter().enumerate() {
            conn.execute(
                &mut add_filed12,
                vec![
                    ("name", Value::String(format!("row{i}"))),
                    ("item", value.clone()),
                ],
            )?;
        }
        let mut add_filed18 = conn.prepare("CREATE (:Filed18 {name: $name, item: $item});")?;
        for (i, value) in filed18_values.iter().enumerate() {
            conn.execute(
                &mut add_filed18,
                vec![
                    ("name", Value::String(format!("row{i}"))),
                    ("item", value.clone()),
                ],
            )?;
        }

        // Every filed-class insert must read back as exactly 0.0000, with
        // the engine's CAST AS STRING agreeing (pinning the stored bytes,
        // not just the read conversion).
        for table in ["Filed12", "Filed18"] {
            let expected_count = if table == "Filed12" { 35 } else { 25 };
            let mut count = 0;
            for result in conn.query(&format!(
                "MATCH (a:{table}) RETURN a.item, CAST(a.item AS STRING);"
            ))? {
                let result = result?;
                assert_eq!(result[0], Value::Decimal(dec!(0.0000)));
                assert_eq!(result[1], Value::String("0.0000".to_string()));
                count += 1;
            }
            assert_eq!(count, expected_count);
        }

        // Half-up tie control: 0.00005 into DECIMAL(12, 4) stores 0.0001,
        // guarding against the fix disturbing ordinary rescale.
        conn.execute(
            &mut add_filed12,
            vec![
                ("name", Value::String("tie".to_string())),
                ("item", Value::Decimal(dec!(0.00005))),
            ],
        )?;
        let result = conn
            .query(
                "MATCH (a:Filed12) WHERE a.name = \"tie\" RETURN a.item, CAST(a.item AS STRING);",
            )?
            .next()
            .unwrap()?;
        assert_eq!(result[0], Value::Decimal(dec!(0.0001)));
        assert_eq!(result[1], Value::String("0.0001".to_string()));

        // No-rescale control: 1e-28 into DECIMAL(38, 28) round-trips
        // exactly (clean pre-fix as well).
        let mut add_control38 = conn.prepare("CREATE (:Control38 {name: $name, item: $item});")?;
        conn.execute(
            &mut add_control38,
            vec![("name", Value::String("tiny".to_string())), ("item", tiny)],
        )?;
        let result = conn
            .query(
                "MATCH (a:Control38) WHERE a.name = \"tiny\" RETURN a.item, CAST(a.item AS STRING);",
            )?
            .next()
            .unwrap()?;
        assert_eq!(
            result[0],
            Value::Decimal(dec!(0.0000000000000000000000000001))
        );
        assert_eq!(
            result[1],
            Value::String("0.0000000000000000000000000001".to_string())
        );
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Tests that reading a stored decimal whose scaled integer exceeds
    /// `rust_decimal`'s capacity (the filed 9.9999999999999999999999999999 in
    /// DECIMAL(38, 28), which `rust_decimal`'s parser rounds to
    /// 10.000000000000000000000000000 at scale 27, so the engine stores
    /// 10.0000000000000000000000000000) surfaces a conversion error instead
    /// of panicking
    fn db_decimal_read_beyond_capacity_precise() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query(
            "CREATE NODE TABLE Person(name STRING, item DECIMAL(38, 28), PRIMARY KEY(name));",
        )?;
        let mut add_person = conn.prepare("CREATE (:Person {name: $name, item: $item});")?;
        conn.execute(
            &mut add_person,
            vec![
                ("name", "Bob".into()),
                (
                    "item",
                    Value::Decimal(
                        "9.9999999999999999999999999999"
                            .parse::<rust_decimal::Decimal>()
                            .expect("filed literal parses, rounding to 10 at scale 27"),
                    ),
                ),
            ],
        )?;
        let as_string = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN CAST(a.item AS STRING);")?
            .next()
            .unwrap()?;
        assert_eq!(
            as_string[0],
            Value::String("10.0000000000000000000000000000".to_string())
        );
        let row = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN a.item;")?
            .next();
        match row {
            Some(Err(ConversionError::Decimal(scaled, err_scale, err))) => {
                assert_eq!(scaled, 100000000000000000000000000000);
                assert_eq!(err_scale, 28);
                assert_eq!(err, rust_decimal::Error::ExceedsMaximumPossibleValue);
            }
            other => panic!("expected Decimal conversion error, got {other:?}"),
        }
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Tests that reading a stored decimal whose scaled integer exceeds
    /// `rust_decimal`'s capacity (`rust_decimal` MAX / 10, bound into
    /// DECIMAL(38, 10), where its scale-10 scaled integer has 38 digits)
    /// surfaces a conversion error instead of panicking
    fn db_decimal_read_beyond_capacity_wide() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query(
            "CREATE NODE TABLE Person(name STRING, item DECIMAL(38, 10), PRIMARY KEY(name));",
        )?;
        let mut add_person = conn.prepare("CREATE (:Person {name: $name, item: $item});")?;
        conn.execute(
            &mut add_person,
            vec![
                ("name", "Bob".into()),
                ("item", Value::Decimal(dec!(7922816251426433759354395033.5))),
            ],
        )?;
        let as_string = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN CAST(a.item AS STRING);")?
            .next()
            .unwrap()?;
        assert_eq!(
            as_string[0],
            Value::String("7922816251426433759354395033.5000000000".to_string())
        );
        let row = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN a.item;")?
            .next();
        match row {
            Some(Err(ConversionError::Decimal(scaled, err_scale, err))) => {
                assert_eq!(scaled, 79228162514264337593543950335000000000);
                assert_eq!(err_scale, 10);
                assert_eq!(err, rust_decimal::Error::ExceedsMaximumPossibleValue);
            }
            other => panic!("expected Decimal conversion error, got {other:?}"),
        }
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Tests that reading an engine-created decimal whose scaled integer
    /// exceeds `rust_decimal`'s capacity (1234567890123456789012345678.0123456789
    /// in DECIMAL(38, 10), created by an engine-side string cast because the
    /// binding cannot originate it) surfaces a conversion error instead of
    /// panicking
    fn db_decimal_read_beyond_capacity_engine_created() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query(
            "CREATE NODE TABLE Person(name STRING, item DECIMAL(38, 10), PRIMARY KEY(name));",
        )?;
        conn.query(
            "CREATE (:Person {name: 'Bob', item: CAST('1234567890123456789012345678.0123456789' AS DECIMAL(38, 10))});",
        )?;
        let as_string = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN CAST(a.item AS STRING);")?
            .next()
            .unwrap()?;
        assert_eq!(
            as_string[0],
            Value::String("1234567890123456789012345678.0123456789".to_string())
        );
        let row = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN a.item;")?
            .next();
        match row {
            Some(Err(ConversionError::Decimal(scaled, err_scale, err))) => {
                assert_eq!(scaled, 12345678901234567890123456780123456789);
                assert_eq!(err_scale, 10);
                assert_eq!(err, rust_decimal::Error::ExceedsMaximumPossibleValue);
            }
            other => panic!("expected Decimal conversion error, got {other:?}"),
        }
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Tests that a beyond-capacity decimal nested in a list surfaces a
    /// conversion error instead of panicking
    fn db_decimal_read_beyond_capacity_in_list() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        let row = conn
            .query(
                "RETURN [CAST('1234567890123456789012345678.0123456789' AS DECIMAL(38, 10))] AS l;",
            )?
            .next();
        match row {
            Some(Err(ConversionError::Decimal(scaled, err_scale, err))) => {
                assert_eq!(scaled, 12345678901234567890123456780123456789);
                assert_eq!(err_scale, 10);
                assert_eq!(err, rust_decimal::Error::ExceedsMaximumPossibleValue);
            }
            other => panic!("expected Decimal conversion error, got {other:?}"),
        }
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Tests that the largest representable decimal (stored scaled integer
    /// equal to `rust_decimal`'s MAX mantissa) and an ordinary small decimal
    /// still read back successfully through the iterator
    fn db_decimal_read_representable_boundary() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query(
            "CREATE NODE TABLE Person(name STRING, item DECIMAL(29, 1), PRIMARY KEY(name));",
        )?;
        let mut add_person = conn.prepare("CREATE (:Person {name: $name, item: $item});")?;
        conn.execute(
            &mut add_person,
            vec![
                ("name", "Bob".into()),
                ("item", Value::Decimal(dec!(7922816251426433759354395033.5))),
            ],
        )?;
        let row = conn
            .query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN a.item;")?
            .next()
            .unwrap()?;
        assert_eq!(row[0], Value::Decimal(dec!(7922816251426433759354395033.5)));
        conn.query("CREATE NODE TABLE Small(name STRING, item DECIMAL(4, 2), PRIMARY KEY(name));")?;
        let mut add_small = conn.prepare("CREATE (:Small {name: $name, item: $item});")?;
        conn.execute(
            &mut add_small,
            vec![
                ("name", "Bob".into()),
                ("item", Value::Decimal(dec!(12.34))),
            ],
        )?;
        let row = conn
            .query("MATCH (a:Small) WHERE a.name = \"Bob\" RETURN a.item;")?
            .next()
            .unwrap()?;
        assert_eq!(row[0], Value::Decimal(dec!(12.34)));
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Tests that the Decimal conversion error's message names the scaled
    /// integer, the scale, and the underlying `rust_decimal` error
    fn decimal_conversion_error_display() {
        let err = ConversionError::Decimal(
            12345678901234567890123456780123456789,
            10,
            rust_decimal::Error::ExceedsMaximumPossibleValue,
        );
        let message = err.to_string();
        assert!(message.contains("12345678901234567890123456780123456789"));
        assert!(message.contains("with scale 10"));
        assert!(message.contains("Number exceeds maximum value that can be represented."));
    }

    fn assert_decimal_eq_with_scale(
        actual: rust_decimal::Decimal,
        expected: rust_decimal::Decimal,
    ) {
        assert_eq!(actual, expected, "numerical value mismatch");
        assert_eq!(actual.scale(), expected.scale(), "scale mismatch");
    }

    #[test]
    /// Tests boundary transitions across `DuckDB` physical storage types:
    /// INT16 (p 1-4) -> INT32 (p 5-9) -> INT64 (p 10-18) -> INT128 (p 19-38)
    fn test_decimal_physical_boundary_transitions() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;

        let boundaries = [
            ("DECIMAL(4, 2)", dec!(99.99), "T4_2"),
            ("DECIMAL(5, 2)", dec!(100.00), "T5_2"),
            ("DECIMAL(9, 4)", dec!(99999.9999), "T9_4"),
            ("DECIMAL(10, 4)", dec!(100000.0000), "T10_4"),
            ("DECIMAL(18, 6)", dec!(999999999999.999999), "T18_6"),
            ("DECIMAL(19, 6)", dec!(1000000000000.000000), "T19_6"),
        ];

        for (col_type, val, table_name) in boundaries {
            conn.query(&format!(
                "CREATE NODE TABLE {table_name}(name STRING, item {col_type}, PRIMARY KEY(name));"
            ))?;
            let mut add_item = conn.prepare(&format!(
                "CREATE (:{table_name} {{name: $name, item: $item}});"
            ))?;
            conn.execute(
                &mut add_item,
                vec![("name", "Bob".into()), ("item", Value::Decimal(val))],
            )?;
            let row = conn
                .query(&format!(
                    "MATCH (a:{table_name}) WHERE a.name = \"Bob\" RETURN a.item;"
                ))?
                .next()
                .unwrap()?;
            if let Value::Decimal(actual) = row[0] {
                assert_decimal_eq_with_scale(actual, val);
            } else {
                panic!("Expected Value::Decimal, got {:?}", row[0]);
            }
        }
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Tests that the list value is correctly constructed
    fn test_list_get() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("testdb"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        for result in conn.query("RETURN [\"Alice\", \"Bob\"] AS l;")? {
            let result = result?;
            assert_eq!(result.len(), 1);
            assert_eq!(
                result[0],
                Value::List(LogicalType::String, vec!["Alice".into(), "Bob".into(),])
            );
        }
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Test that the timestamp round-trips through lbug's internal timestamp
    fn test_timestamp() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("test"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query(
            "CREATE NODE TABLE Person(name STRING, registerTime TIMESTAMP, PRIMARY KEY(name));",
        )?;
        conn.query(
            "CREATE (:Person {name: \"Alice\", registerTime: timestamp(\"2011-08-20 11:25:30\")});",
        )?;
        let mut add_person =
            conn.prepare("CREATE (:Person {name: $name, registerTime: $time});")?;
        let timestamp = datetime!(2011-08-20 11:25:30 UTC);
        conn.execute(
            &mut add_person,
            vec![
                ("name", "Bob".into()),
                ("time", Value::Timestamp(timestamp)),
            ],
        )?;
        let result: HashSet<String> = conn
            .query(
                "MATCH (a:Person) WHERE a.registerTime = timestamp(\"2011-08-20 11:25:30\") RETURN a.name;",
            )?
            .map(|x| match &x.unwrap()[0] {
                Value::String(x) => x.clone(),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            result,
            HashSet::from_iter(vec!["Alice".to_string(), "Bob".to_string()])
        );
        let mut result =
            conn.query("MATCH (a:Person) WHERE a.name = \"Bob\" RETURN a.registerTime;")?;
        let result: time::OffsetDateTime =
            if let Value::Timestamp(timestamp) = result.next().unwrap()?[0] {
                timestamp
            } else {
                panic!("Wrong type returned!")
            };
        assert_eq!(result, timestamp);
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    fn test_node() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("test"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name));")?;
        conn.query("CREATE (:Person {name: \"Alice\", age: 25});")?;
        let result = conn.query("MATCH (a:Person) RETURN a;")?.next().unwrap()?;
        assert_eq!(
            result[0],
            Value::Node(NodeVal {
                id: InternalID {
                    table_id: 0,
                    offset: 0
                },
                label: "Person".to_string(),
                properties: vec![
                    ("name".to_string(), Value::String("Alice".to_string())),
                    ("age".to_string(), Value::Int64(25))
                ]
            })
        );
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    fn test_recursive_rel() -> Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let db = Database::new(temp_dir.path().join("test"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name));")?;
        conn.query("CREATE REL TABLE knows(FROM Person TO Person);")?;
        conn.query("CREATE (:Person {name: \"Alice\", age: 25});")?;
        conn.query("CREATE (:Person {name: \"Bob\", age: 25});")?;
        conn.query("CREATE (:Person {name: \"Eve\", age: 25});")?;
        conn.query(
            "MATCH (p1:Person), (p2:Person)
                WHERE p1.name = \"Alice\" AND p2.name = \"Bob\"
            CREATE (p1)-[:knows]->(p2);",
        )?;
        conn.query(
            "MATCH (p1:Person), (p2:Person)
                WHERE p1.name = \"Bob\" AND p2.name = \"Eve\"
            CREATE (p1)-[:knows]->(p2);",
        )?;
        let result = conn
            .query(
                "MATCH (a:Person)-[e*2..2]->(b:Person)
                 WHERE a.name = 'Alice'
                 RETURN e, b.name;",
            )?
            .next()
            .unwrap()?;
        assert_eq!(result[1], Value::String("Eve".to_string()));
        assert_eq!(
            result[0],
            Value::RecursiveRel {
                nodes: vec![NodeVal {
                    id: (1, 0).into(),
                    label: "Person".into(),
                    properties: vec![("name".into(), "Bob".into()), ("age".into(), 25i64.into())]
                },],
                rels: vec![
                    RelVal::new((0, 0), (1, 0), "knows"),
                    RelVal::new((1, 0), (2, 0), "knows"),
                ],
            }
        );
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Test that null values are read correctly by the API
    fn test_null() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("test"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        let result = conn.query("RETURN null")?.next().transpose()?;
        let result = &result.unwrap()[0];
        assert_eq!(result, &Value::Null(LogicalType::String));
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Tests that passing the values through the database returns what we put in
    fn test_serial() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("test"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query("CREATE NODE TABLE Person(id SERIAL, name STRING, PRIMARY KEY(id));")?;
        conn.query("CREATE (:Person {name: \"Bob\"});")?;
        conn.query("CREATE (:Person {name: \"Alice\"});")?;
        let result = conn.query("MATCH (a:Person) RETURN a.name, a.id;")?;
        assert_eq!(
            result.get_column_data_types(),
            vec![LogicalType::String, LogicalType::Serial]
        );
        let results: Vec<(Value, Value)> = result
            .map(|x| {
                let mut x = x.unwrap();
                (x.pop().unwrap(), x.pop().unwrap())
            })
            .collect();
        assert_eq!(
            results,
            vec![
                (Value::Int64(0), "Bob".into()),
                (Value::Int64(1), "Alice".into())
            ]
        );
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    /// Tests that passing the values through the database returns what we put in
    fn test_union() -> Result<()> {
        use std::fs::File;
        use std::io::Write;
        let temp_dir = tempfile::tempdir()?;
        let db = Database::new(temp_dir.path().join("test"), SYSTEM_CONFIG_FOR_TESTS)?;
        let conn = Connection::new(&db)?;
        conn.query(
            "CREATE NODE TABLE demo(a SERIAL, b UNION(num INT64, str STRING), PRIMARY KEY(a));",
        )?;
        let mut file = File::create(temp_dir.path().join("demo.csv"))?;
        file.write_all(b"1\naa\n")?;
        conn.query(&format!(
            "COPY demo from '{}/demo.csv';",
            // Use forward-slashes instead of backslashes on windows, as thmay not be supported by
            // the query parser
            temp_dir.path().display().to_string().replace('\\', "/")
        ))?;
        let result = conn.query("MATCH (d:demo) RETURN d.b;")?;
        let types = vec![
            ("num".to_string(), LogicalType::Int64),
            ("str".to_string(), LogicalType::String),
        ];
        assert_eq!(
            result.get_column_data_types(),
            vec![LogicalType::Union {
                types: types.clone()
            }],
        );
        let results: Vec<Value> = result.map(|x| x.unwrap().pop().unwrap()).collect();
        assert_eq!(
            results,
            vec![
                Value::Union {
                    types: types.clone(),
                    value: Box::new(Value::Int64(1))
                },
                Value::Union {
                    types: types.clone(),
                    value: Box::new(Value::String("aa".to_string()))
                },
            ]
        );
        temp_dir.close()?;
        Ok(())
    }
}
