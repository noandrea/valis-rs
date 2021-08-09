# VALIS

:warning: Early stage prototype :warning:

[![Crates.io](https://img.shields.io/crates/v/valis)](https://crates.io/crates/valis)
[![Coverage Status](https://coveralls.io/repos/github/noandrea/valis-rs/badge.svg?branch=master)](https://coveralls.io/github/noandrea/valis-rs?branch=master)
[![dependency status](https://deps.rs/repo/github/noandrea/valis-rs/status.svg)](https://deps.rs/repo/github/noandrea/valis-rs)

This is the [VALIS](https://meetvalis.com) library and command line client.

It allows to maintain a VALIS landscape locally using your terminal


## Installation 

install it with `cargo install valis`

## Data Model

Valis data model consists of two main constructs: `Entities` and `Events`.

An e


### Entities

Entities are the principal construct that the user interacts with. Entities are everything that as a life that persist in time, such as people, physical objects, projects and organizations.

An entity is identified by two main attributes that are:

- `name` - the human readable name of the entity 
- `kind` - the type of the entity, that is `person`, `object`, `abstract` 

> TODO: only entities with autonomous operations should be sponsor of another entity 

- Relationship Quality

#### Sponsorship

Sponsorship is one of the fundamental properties of the entity and indicates which other entity is responsible to take care of an entity.



#### Relationships

Relationships are explicit connections between entities. Example of relationships are 

- `fatherOf`
- `foundedBy`
- `employee`
- `...`

> are one way or many ways?


##### Other types

**`RelState`**  indicates the status of the entity in the general context, possible values are

- `Root`, this would be the center of the application
- `Active` (NaiveDate, Option<NaiveDate>)  - the entity is active, the normal interactive status of an entity
- `Passive`(NaiveDate, Option<NaiveDate>) - the entity is passive, it does not have an active role in the context but it lives in the background. (eg. )
- `Former`(NaiveDate, Option<NaiveDate>) - there use to be an active relationship but it has been terminated (e.g. former collaborators)
- `Disabled`(NaiveDate, Option<NaiveDate>) - the entity are stored only for auditing and historical purposes but they do not "exists" anymore 

### Events

Events in VALIS are the threads that connects entities and allow to record the temporal coordinate of the dataset and to track hidden patterns of communications and interactions .

There are two kind of events, log events and action events. **Log events** are generated for auditing purposes (entity created and so on); **action events** are generated during the lifetime of an entity, and specifically when a `next_action_note` and/or `next_action_date` are updated.

> The delay action: when the action toward an entity is delayed, an action event `delay` is recorded. This event is then used to *health check* the status of an Entity (eg. delayed too much )


**Actors**

Each event have one or more actor, an actor is an [Entity](#Entities) that participates to the 
event wit a role. The actors roles are:

- `RecordedBy`  - notes, meeting transcript, etc
- `Subject`  - notes about something/someone
- `Lead`  - meetings, events, etc
- `Starring`  - entities attending
- `Background`  - the context within the 

### 

---
Made by [adgb](https://adgb.me)



