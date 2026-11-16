# Externally supplied resource cases

These supplement the schema-comparison profile. Fixture format 1.0 has no supplied-resource carrier, so these cases are outside manifest.json and do not add a fixture field. Each comparison is directional input: target values flow to the candidate. Supply the listed resource map to the resolver without network access.

Schemas contained in an OpenBindings document are read as 2020-12 under specification section 5.5; a different declaration violates OBI-09. The five cases below concern schema positions reached through external URIs; the cases under [Documents that are not OBI documents](#documents-that-are-not-obi-documents) concern a document handed to comparison itself. Apply the dialect JSON Schema assigns each position, including inheritance and embedded resources. The profile deliberately answers indeterminate on a non-identical externally reached foreign-dialect position on either side.

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

## Documents that are not OBI documents

These cases hand comparison a document that is not an OBI document: none has the `openbindings` and `operations` members an OBI document requires. Each gives that document (`document`), the JSON Pointer of the compared position within it (`pointer`, empty for the document root), a candidate schema with no `$schema`, read as 2020-12, and the resources supplied to the resolver (`resources`). The comparison is directional input, as above, with no base URI. The document keeps the dialect JSON Schema assigns it: the `$schema` its resource root declares or inherits from an enclosing resource, or 2020-12 where none is declared. A position whose dialect is not 2020-12 follows the externally assigned dialect rule, so the profile answers indeterminate there unless the two sides are identical.

### non-obi-undeclared-root

```json
{
  "document": {
    "$ref": "#/$defs/s",
    "maxLength": 1,
    "$defs": {
      "s": {
        "type": "string"
      }
    }
  },
  "pointer": "",
  "candidate": {
    "type": "string",
    "maxLength": 1
  },
  "resources": {}
}
```

Expected profile verdict: **compatible**. The document declares no `$schema`, so it is read as 2020-12: the reference's sibling `maxLength` applies, and the position normalizes to the same schema as the candidate, so identity proves compatible. Read as draft-07, the sibling would be ignored and "ab" admitted.

### non-obi-draft-07-root

```json
{
  "document": {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "$ref": "#/definitions/s",
    "maxLength": 1,
    "definitions": {
      "s": {
        "type": "string"
      }
    }
  },
  "pointer": "",
  "candidate": {
    "type": "string",
    "maxLength": 1
  },
  "resources": {}
}
```

Expected profile verdict: **indeterminate**. Actual inclusion fails: draft-07 ignores the siblings of `$ref`, so "ab" is admitted by the target and refused by the candidate.

### non-obi-draft-07-non-keyword-member

```json
{
  "document": {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "schemas": {
      "A": {
        "$ref": "https://r.test/s",
        "maxLength": 1
      }
    }
  },
  "pointer": "/schemas/A",
  "candidate": {
    "type": "string",
    "maxLength": 1
  },
  "resources": {
    "https://r.test/s": {
      "type": "string"
    }
  }
}
```

Expected profile verdict: **indeterminate**. `schemas` is not a draft-07 keyword, but the position under it that the pointer reaches still lies within the draft-07 root resource; the dialect of the resource its reference reaches does not change that. Actual inclusion fails: "ab" is admitted by the target and refused by the candidate.

### non-obi-draft-07-inherited-resource

```json
{
  "document": {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "definitions": {
      "p": {
        "$id": "https://r.test/inner",
        "allOf": [
          {
            "$ref": "#/definitions/s",
            "maxLength": 1
          }
        ],
        "definitions": {
          "s": {
            "type": "string"
          }
        }
      }
    }
  },
  "pointer": "/definitions/p",
  "candidate": {
    "type": "string",
    "maxLength": 1
  },
  "resources": {}
}
```

Expected profile verdict: **indeterminate**. The embedded resource declares no `$schema` of its own and inherits draft-07 from the document root; its reference resolves within it. Actual inclusion fails: "ab" is admitted by the target and refused by the candidate.
