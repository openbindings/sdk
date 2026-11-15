# Externally supplied resource cases

These supplement the schema-comparison profile. Fixture format 1.0 has no supplied-resource carrier, so these cases are outside manifest.json and do not add a fixture field. Each comparison is directional input: target values flow to the candidate. Supply the listed resource map to the resolver without network access.

Schemas contained in an OpenBindings document are read as 2020-12 under specification section 5.5; a different declaration violates OBI-09. The cases below concern only schema positions reached through external URIs. Apply the dialect JSON Schema assigns each position, including inheritance and embedded resources. The profile deliberately answers indeterminate on a non-identical externally reached foreign-dialect position on either side.

## foreign-dialect-root

```json
{
  "target": {
    "$ref": "https://r.test/a"
  },
  "candidate": {
    "type": "string"
  },
  "resources": {
    "https://r.test/a": {
      "$schema": "http://json-schema.org/draft-07/schema#",
      "$ref": "https://r.test/any",
      "type": "string"
    },
    "https://r.test/any": {}
  }
}
```

Expected profile verdict: **indeterminate**. Actual inclusion fails: -0.5 is admitted by the target and refused by the candidate.

## foreign-dialect-fragment

```json
{
  "target": {
    "$ref": "https://r.test/a#/definitions/d"
  },
  "candidate": {
    "type": "string"
  },
  "resources": {
    "https://r.test/a": {
      "$schema": "http://json-schema.org/draft-07/schema#",
      "definitions": {
        "d": {
          "$ref": "https://r.test/any",
          "type": "string"
        }
      }
    },
    "https://r.test/any": {}
  }
}
```

Expected profile verdict: **indeterminate**. Actual inclusion fails: -0.5 is admitted by the target and refused by the candidate.

## foreign-dialect-receiving

```json
{
  "target": {
    "type": "string"
  },
  "candidate": {
    "$ref": "https://r.test/c"
  },
  "resources": {
    "https://r.test/c": {
      "$schema": "http://json-schema.org/draft-07/schema#",
      "$ref": "https://r.test/any",
      "type": "integer"
    },
    "https://r.test/any": {}
  }
}
```

Expected profile verdict: **indeterminate**. Actual inclusion holds: the receiving draft-07 reference ignores its integer sibling and admits every string; "a" is an example.

## foreign-dialect-inherited-resource

```json
{
  "target": {
    "$ref": "https://r.test/inner"
  },
  "candidate": {
    "type": "string"
  },
  "resources": {
    "https://r.test/a": {
      "$schema": "http://json-schema.org/draft-07/schema#",
      "properties": {
        "p": {
          "$id": "https://r.test/inner",
          "allOf": [
            {
              "$ref": "https://r.test/any",
              "type": "string"
            }
          ]
        }
      }
    },
    "https://r.test/any": {}
  }
}
```

Expected profile verdict: **indeterminate**. Actual inclusion fails: -0.5 is admitted by the target and refused by the candidate.

The inner resource has no declaration of its own and inherits draft-07.

## foreign-dialect-nested-declaration

```json
{
  "target": {
    "$ref": "https://r.test/a#/properties/p"
  },
  "candidate": {
    "type": "string"
  },
  "resources": {
    "https://r.test/a": {
      "$schema": "https://json-schema.org/draft/2020-12/schema",
      "properties": {
        "p": {
          "$id": "https://r.test/inner",
          "$schema": "http://json-schema.org/draft-07/schema#",
          "$ref": "https://r.test/any",
          "type": "string"
        }
      }
    },
    "https://r.test/any": {}
  }
}
```

Expected profile verdict: **indeterminate**. Actual inclusion fails: -0.5 is admitted by the target and refused by the candidate.
