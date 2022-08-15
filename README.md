# Reserved

## Session

- list adapters
- get adapter
- default adapter
- events stream (adapter added/removed)

### Adapter

- get known devices
- request device
- events (availability changed)

#### Device

- id
- name
- connected
- connect
- disconnect
- get primary service(s)

##### GATT Service

- is_primary
- uuid
- get characteristics

###### GATT Characteristic

- properties
- service
- uuid
- value
- events (value changed)
- get descriptor(s)
- read_Value
- write_value_With_response
- write_value_Without_response
- start/stop notifications

###### GATT Descriptor

- characteristic
- uuid
- value
- read value
- write value
