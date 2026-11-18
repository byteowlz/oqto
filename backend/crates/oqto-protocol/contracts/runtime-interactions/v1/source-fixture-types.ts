// Generated source frame shape checks; not authenticated host admission.
import type { RuntimeSourceFrame } from './RuntimeSourceFrame';
export const frames = [
  {
    "request": {
      "operation": {
        "op": "capabilities"
      },
      "requestId": "source-request-0",
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source"
  },
  {
    "response": {
      "requestId": "source-request-0",
      "result": {
        "capabilities": [
          {
            "family": "commands",
            "reason": "sdk_engine_absent",
            "state": "unavailable"
          }
        ],
        "kind": "capabilities"
      },
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source_response"
  },
  {
    "request": {
      "operation": {
        "op": "command_catalog"
      },
      "requestId": "source-request-1",
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source"
  },
  {
    "response": {
      "requestId": "source-request-1",
      "result": {
        "catalog": {
          "commands": [
            {
              "arguments": {
                "kind": "raw_string"
              },
              "availability": {
                "state": "available"
              },
              "commandId": "fixture-command",
              "description": null,
              "invocation": "/fixture",
              "origin": "builtin",
              "requiredInteractions": []
            }
          ],
          "coverage": "full",
          "generation": "fixture-catalog"
        },
        "kind": "catalog"
      },
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source_response"
  },
  {
    "request": {
      "operation": {
        "args": "  --value='literal /name' \n",
        "catalogGeneration": "fixture-catalog",
        "commandId": "fixture-command",
        "op": "command_dispatch"
      },
      "requestId": "source-request-2",
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source"
  },
  {
    "response": {
      "requestId": "source-request-2",
      "result": {
        "kind": "command_receipt",
        "receipt": {
          "commandId": "fixture-command",
          "disposition": "started",
          "operationId": "fixture-operation",
          "rejection": null
        }
      },
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source_response"
  },
  {
    "request": {
      "operation": {
        "op": "permission_snapshot"
      },
      "requestId": "source-request-3",
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source"
  },
  {
    "response": {
      "requestId": "source-request-3",
      "result": {
        "kind": "permissions",
        "snapshot": {
          "active": [
            {
              "ceilingDigest": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
              "expiresAtMs": 10000,
              "handleId": "fixture-handle",
              "operationDigest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
              "options": [
                {
                  "effectDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                  "model": "fixture/alternate",
                  "optionId": "allow"
                },
                {
                  "effectDigest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                  "model": null,
                  "optionId": "deny"
                }
              ],
              "requestId": "fixture-permission-request",
              "requestRevision": 0,
              "subject": {
                "effectDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "kind": "pi",
                "loadout": null,
                "model": "fixture/alternate"
              },
              "subjectDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }
          ],
          "cursor": {
            "generation": "fixture-runtime",
            "revision": 4
          },
          "receipts": [
            {
              "atMs": 10000,
              "controllerId": "fixture-controller",
              "cursor": {
                "generation": 12,
                "revision": 4
              },
              "handleId": "fixture-handle",
              "kind": "decision",
              "nativeSessionDigest": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
              "operationDigest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
              "optionId": "allow",
              "outcome": "allow",
              "reason": "allowed",
              "requestId": "fixture-permission-request",
              "requestRevision": 0,
              "restored": false,
              "source": "adapter",
              "subjectDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }
          ]
        }
      },
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source_response"
  },
  {
    "request": {
      "operation": {
        "controllerGeneration": "fixture-controller-generation",
        "controllerId": "fixture-controller",
        "decision": "allow_once",
        "handleId": "fixture-handle",
        "op": "permission_decide",
        "optionId": "allow",
        "producerGeneration": "fixture-runtime",
        "requestRevision": 0
      },
      "requestId": "source-request-4",
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source"
  },
  {
    "response": {
      "requestId": "source-request-4",
      "result": {
        "kind": "permission_receipt",
        "receipt": {
          "atMs": 10000,
          "controllerId": "fixture-controller",
          "cursor": {
            "generation": 12,
            "revision": 4
          },
          "handleId": "fixture-handle",
          "kind": "decision",
          "nativeSessionDigest": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
          "operationDigest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
          "optionId": "allow",
          "outcome": "allow",
          "reason": "allowed",
          "requestId": "fixture-permission-request",
          "requestRevision": 0,
          "restored": false,
          "source": "adapter",
          "subjectDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }
      },
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source_response"
  },
  {
    "request": {
      "operation": {
        "op": "policy_read"
      },
      "requestId": "source-request-5",
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source"
  },
  {
    "response": {
      "requestId": "source-request-5",
      "result": {
        "kind": "policy",
        "policy": {
          "allowMode": "confirm",
          "allowedKinds": [
            "pi"
          ],
          "allowedModels": [
            "fixture/model"
          ],
          "autoDecision": "deny",
          "confirmTimeoutMs": 10000,
          "cursor": {
            "generation": "fixture-runtime",
            "revision": 4
          },
          "maxSubagents": 2
        }
      },
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source_response"
  },
  {
    "request": {
      "operation": {
        "changes": [
          {
            "setting": "allowed_models",
            "value": [
              "fixture/model"
            ]
          }
        ],
        "expected": {
          "generation": "fixture-runtime",
          "revision": 4
        },
        "op": "policy_apply"
      },
      "requestId": "source-request-6",
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source"
  },
  {
    "response": {
      "requestId": "source-request-6",
      "result": {
        "kind": "policy",
        "policy": {
          "allowMode": "confirm",
          "allowedKinds": [
            "pi"
          ],
          "allowedModels": [
            "fixture/model"
          ],
          "autoDecision": "deny",
          "confirmTimeoutMs": 10000,
          "cursor": {
            "generation": "fixture-runtime",
            "revision": 4
          },
          "maxSubagents": 2
        }
      },
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source_response"
  },
  {
    "request": {
      "operation": {
        "op": "queue_snapshot"
      },
      "requestId": "source-request-7",
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source"
  },
  {
    "response": {
      "requestId": "source-request-7",
      "result": {
        "kind": "queue",
        "snapshot": {
          "completeness": "complete",
          "cursor": {
            "generation": "fixture-runtime",
            "revision": 4
          },
          "followUp": [],
          "steering": [
            {
              "itemId": "fixture-q1",
              "text": "same text"
            },
            {
              "itemId": "fixture-q2",
              "text": "same text"
            }
          ],
          "supportedActions": [
            "steer",
            "follow_up",
            "cancel_item"
          ]
        }
      },
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source_response"
  },
  {
    "request": {
      "operation": {
        "action": {
          "action": "cancel_item",
          "itemId": "fixture-q1"
        },
        "expected": {
          "generation": "fixture-runtime",
          "revision": 4
        },
        "op": "queue_apply"
      },
      "requestId": "source-request-8",
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source"
  },
  {
    "response": {
      "requestId": "source-request-8",
      "result": {
        "kind": "queue",
        "snapshot": {
          "completeness": "complete",
          "cursor": {
            "generation": "fixture-runtime",
            "revision": 4
          },
          "followUp": [],
          "steering": [
            {
              "itemId": "fixture-q1",
              "text": "same text"
            },
            {
              "itemId": "fixture-q2",
              "text": "same text"
            }
          ],
          "supportedActions": [
            "steer",
            "follow_up",
            "cancel_item"
          ]
        }
      },
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source_response"
  },
  {
    "request": {
      "operation": {
        "afterEventId": null,
        "op": "error_list"
      },
      "requestId": "source-request-9",
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source"
  },
  {
    "response": {
      "requestId": "source-request-9",
      "result": {
        "kind": "errors",
        "snapshot": {
          "cursor": {
            "generation": "fixture-runtime",
            "revision": 4
          },
          "errors": [
            {
              "code": "fixture_error",
              "eventId": "fixture-error",
              "message": "Safe synthetic failure",
              "operationId": "fixture-operation",
              "phase": "dispatch",
              "recoverable": true,
              "source": "extension",
              "uncertain": false
            }
          ]
        }
      },
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source_response"
  },
  {
    "request": {
      "operation": {
        "op": "receipt_get",
        "operationId": "fixture-operation"
      },
      "requestId": "source-request-10",
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source"
  },
  {
    "response": {
      "requestId": "source-request-10",
      "result": {
        "kind": "command_receipt",
        "receipt": {
          "commandId": "fixture-command",
          "disposition": "started",
          "operationId": "fixture-operation",
          "rejection": null
        }
      },
      "source": {
        "nativeSessionId": "native-fixture",
        "runtimeGeneration": "ir:11111111-1111-4111-8111-111111111111"
      },
      "version": 1
    },
    "type": "interaction_source_response"
  },
  {
    "code": "invalid_request",
    "requestId": null,
    "type": "interaction_source_status"
  },
  {
    "code": "not_negotiated",
    "requestId": "source-request-0",
    "type": "interaction_source_status"
  },
  {
    "code": "source_unavailable",
    "requestId": "source-request-0",
    "type": "interaction_source_status"
  },
  {
    "code": "source_changed",
    "requestId": "source-request-0",
    "type": "interaction_source_status"
  },
  {
    "code": "authority_denied",
    "requestId": "source-request-0",
    "type": "interaction_source_status"
  },
  {
    "code": "unsupported_operation",
    "requestId": "source-request-0",
    "type": "interaction_source_status"
  }
] satisfies RuntimeSourceFrame[];
// @ts-expect-error Host claims cannot enter native source frames.
const invalidSource0: RuntimeSourceFrame = {"request":{"binding":{"controllerGeneration":"fixture-controller-generation","controllerId":"fixture-controller","nativeSessionId":"native-fixture","publicSessionId":"oqto-fixture","runtimeGeneration":"fixture-runtime"},"operation":{"op":"capabilities"},"requestId":"source-request-0","source":{"nativeSessionId":"native-fixture","runtimeGeneration":"ir:11111111-1111-4111-8111-111111111111"},"version":1},"type":"interaction_source"};
// @ts-expect-error Host claims cannot enter native source frames.
const invalidSource1: RuntimeSourceFrame = {"response":{"binding":{"controllerGeneration":"fixture-controller-generation","controllerId":"fixture-controller","nativeSessionId":"native-fixture","publicSessionId":"oqto-fixture","runtimeGeneration":"fixture-runtime"},"requestId":"source-request-0","result":{"capabilities":[{"family":"commands","reason":"sdk_engine_absent","state":"unavailable"}],"kind":"capabilities"},"source":{"nativeSessionId":"native-fixture","runtimeGeneration":"ir:11111111-1111-4111-8111-111111111111"},"version":1},"type":"interaction_source_response"};
// @ts-expect-error Host claims cannot enter native source frames.
const invalidSource2: RuntimeSourceFrame = {"request":{"operation":{"op":"capabilities"},"requestId":"source-request-0","source":{"nativeSessionId":"native-fixture","publicSessionId":"forged","runtimeGeneration":"ir:11111111-1111-4111-8111-111111111111"},"version":1},"type":"interaction_source"};
