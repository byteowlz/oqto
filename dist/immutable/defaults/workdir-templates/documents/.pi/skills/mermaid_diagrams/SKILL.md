---
name: mermaid_diagrams
description: Create valid code for generating mermaid diagrams
license: MIT
---

The user may ask you to generate mermaid diagrams. This is a detailed explanation on how to create these diagrams.

## Instructions

Generate valid Mermaid code only. Use the requested diagram type. Reflect the described structure with correct syntax, labels, and links. Do not include explanations or markdown unless requested.

## Input Requirements

- Description: Natural language description of the diagram
- Diagram Type: One of: flowchart, sequence, class, state, er, gantt

## Output Requirement

- Valid Mermaid diagram code only
- You save diagrams .mmd files
- You can use the mermaid CLI to validate if code is valid: mmdc -i <input_file.mmd> -o <output_file.svg_or_png>

## Examples

### Example 1

Description: Simple decision flowchart with start, decision, actions, and end
Type: flowchart
Mermaid Code:

```mermaid
flowchart TD
    A[Start] --> B{Decision?}
    B -->|Yes| C[Action 1]
    B -->|No| D[Action 2]
    C --> E[End]
    D --> E
```

### Example 2

Description: Left-to-right flowchart showing a simple process flow
Type: flowchart
Mermaid Code:

```mermaid
flowchart LR
    A[Input] --> B[Process]
    B --> C[Output]
    C --> D[Validation]
    D -->|Valid| E[Save]
    D -->|Invalid| B
```

### Example 3

Description: User authentication sequence diagram showing interactions between user, frontend, and backend
Type: sequence
Mermaid Code:

```mermaid
sequenceDiagram
    participant U as User
    participant F as Frontend
    participant B as Backend

    U->>F: Enter credentials
    F->>B: Login request
    B-->>F: Authentication token
    F-->>U: Login successful
```

### Example 4

Description: API call sequence with error handling
Type: sequence
Mermaid Code:

```mermaid
sequenceDiagram
    participant Client
    participant API
    participant DB

    Client->>API: Request data
    API->>DB: Query database
    alt Success
        DB-->>API: Return data
        API-->>Client: Response with data
    else Error
        DB-->>API: Error message
        API-->>Client: Error response
    end
```

### Example 5

Description: Object-oriented class diagram showing inheritance and relationships
Type: class
Mermaid Code:

```mermaid
classDiagram
    Animal <|-- Dog
    Animal <|-- Cat
    Animal : +String name
    Animal : +int age
    Animal : +makeSound()

    class Dog {
        +String breed
        +bark()
    }

    class Cat {
        +String color
        +meow()
    }
```

### Example 6

Description: State machine for a simple order processing system
Type: state
Mermaid Code:

```mermaid
stateDiagram-v2
    [*] --> Pending
    Pending --> Processing : payment_received
    Processing --> Shipped : items_dispatched
    Processing --> Cancelled : payment_failed
    Shipped --> Delivered : delivery_confirmed
    Delivered --> [*]
    Cancelled --> [*]
```
