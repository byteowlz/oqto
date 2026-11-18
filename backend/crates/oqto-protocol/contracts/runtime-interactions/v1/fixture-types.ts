// Generated static shape checks; schema/serde enforce runtime bounds.
import type { RuntimeRequest } from './RuntimeRequest';
export const requests = [
  {
    "operation": {
      "op": "capabilities"
    },
    "requestId": "fixture-request",
    "version": 1
  },
  {
    "operation": {
      "op": "command_catalog"
    },
    "requestId": "fixture-request",
    "version": 1
  },
  {
    "operation": {
      "args": "  --value='literal /name' \n",
      "catalogGeneration": "fixture-catalog",
      "commandId": "fixture-command",
      "op": "command_dispatch"
    },
    "requestId": "fixture-request",
    "version": 1
  },
  {
    "operation": {
      "op": "permission_snapshot"
    },
    "requestId": "fixture-request",
    "version": 1
  },
  {
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
    "requestId": "fixture-request",
    "version": 1
  },
  {
    "operation": {
      "op": "policy_read"
    },
    "requestId": "fixture-request",
    "version": 1
  },
  {
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
    "requestId": "fixture-request",
    "version": 1
  },
  {
    "operation": {
      "op": "queue_snapshot"
    },
    "requestId": "fixture-request",
    "version": 1
  },
  {
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
    "requestId": "fixture-request",
    "version": 1
  },
  {
    "operation": {
      "afterEventId": null,
      "op": "error_list"
    },
    "requestId": "fixture-request",
    "version": 1
  },
  {
    "operation": {
      "op": "receipt_get",
      "operationId": "fixture-operation"
    },
    "requestId": "fixture-request",
    "version": 1
  }
] satisfies RuntimeRequest[];
import type { RuntimeResponse } from './RuntimeResponse';
export const responses = [
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "requestId": "fixture-request",
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
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "requestId": "fixture-request",
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
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "requestId": "fixture-request",
    "result": {
      "kind": "command_receipt",
      "receipt": {
        "commandId": "fixture-command",
        "disposition": "started",
        "operationId": "fixture-operation",
        "rejection": null
      }
    },
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "requestId": "fixture-request",
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
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "requestId": "fixture-request",
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
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "requestId": "fixture-request",
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
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "requestId": "fixture-request",
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
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "requestId": "fixture-request",
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
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "requestId": "fixture-request",
    "result": {
      "family": "queue",
      "kind": "unavailable",
      "reason": "sdk_engine_absent"
    },
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "requestId": "fixture-request",
    "result": {
      "code": "stale_generation",
      "kind": "rejected"
    },
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": 2,
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": 12
    },
    "requestId": "fixture-request",
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
    "version": 1
  }
] satisfies RuntimeResponse[];
import type { RuntimeEvent } from './RuntimeEvent';
export const events = [
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "eventId": "fixture-event",
    "payload": {
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
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "eventId": "fixture-event",
    "payload": {
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
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "eventId": "fixture-event",
    "payload": {
      "error": {
        "code": "fixture_error",
        "eventId": "fixture-error",
        "message": "Safe synthetic failure",
        "operationId": "fixture-operation",
        "phase": "dispatch",
        "recoverable": true,
        "source": "extension",
        "uncertain": false
      },
      "kind": "error"
    },
    "version": 1
  },
  {
    "binding": {
      "controllerGeneration": "fixture-controller-generation",
      "controllerId": "fixture-controller",
      "nativeSessionId": "native-fixture",
      "publicSessionId": "oqto-fixture",
      "runtimeGeneration": "fixture-runtime"
    },
    "eventId": "fixture-event",
    "payload": {
      "errorId": "fixture-error",
      "kind": "command_outcome",
      "operationId": "fixture-operation",
      "state": "uncertain"
    },
    "version": 1
  }
] satisfies RuntimeEvent[];
// @ts-expect-error Invalid canonical request shape.
const invalid1: RuntimeRequest = {"operation":{"op":"capabilities"},"requestId":"fixture-request","version":2};
// @ts-expect-error Invalid canonical request shape.
const invalid4: RuntimeRequest = {"binding":{"controllerGeneration":"fixture-controller-generation","controllerId":"fixture-controller","nativeSessionId":"native-fixture","publicSessionId":"oqto-fixture","runtimeGeneration":"fixture-runtime"},"operation":{"op":"capabilities"},"requestId":"fixture-request","version":1};
// @ts-expect-error Invalid canonical request shape.
const invalid5: RuntimeRequest = {"operation":{"message":"/fixture","op":"prompt"},"requestId":"fixture-request","version":1};
// @ts-expect-error Invalid canonical request shape.
const invalid6: RuntimeRequest = {"operation":{"args":"  --value='literal /name' \n","commandId":"fixture-command","op":"command_dispatch"},"requestId":"fixture-request","version":1};
// @ts-expect-error Invalid canonical request shape.
const invalid10: RuntimeRequest = {"operation":{"controllerGeneration":"fixture-controller-generation","controllerId":"fixture-controller","decision":"allow_once","handleId":"fixture-handle","model":"forged/model","op":"permission_decide","optionId":"allow","producerGeneration":"fixture-runtime","requestRevision":0},"requestId":"fixture-request","version":1};
