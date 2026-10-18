# SNCF connector

Official SNCF API (built on [Navitia](https://doc.navitia.io)). Read-only.

## Setup

1. Get a free key: <https://numerique.sncf.com/startup/api/>.
2. Expose it to autocruise as `SNCF_API_TOKEN`.

The key is sent as the `Authorization` header (equivalent to HTTP basic auth
with the key as user name and an empty password).

## Tools

| Tool | What it does |
|---|---|
| `sncf_search_places` | Find stations (and cities, addresses, POIs) by name → ids |
| `sncf_nearby_stations` | Stations around a latitude/longitude |
| `sncf_plan_journey` | Journeys from A to B, departing after / arriving before a time, with transfers, delays and disruptions |
| `sncf_departures` | Departure board of a station, real-time delays |
| `sncf_arrivals` | Arrival board of a station, real-time delays |
| `sncf_train` | One train by number and date: all stops and times |
| `sncf_disruptions` | Active/upcoming disruptions for a station, or network-wide for a day |

Places can be given as ids (`stop_area:SNCF:87391003`), as `latitude,longitude`,
or as free text. Free text resolves to an exact name match first, then a name
starting with the query, then the API's top result. (The API ranks fuzzily:
"Lyon" returns "Paris - Gare de Lyon" first.) Outputs always echo the resolved place.

## Time zones (verified against the live API, 2026-10-18)

- Responses are **Europe/Paris local time without offset** (`20261019T080600`).
  The response `context` states `"timezone": "Europe/Paris"`.
- Request date-times **without offset** are read as Europe/Paris.
- Request date-times with `Z` or `+00:00` are converted from UTC by the API:
  `20261019T120000Z` returns trains from 14:00 local in summer time.
- `departures` / `arrivals` take **`from_datetime`**. A `datetime` parameter
  is **silently ignored** there (the board then starts "now").
- `vehicle_journeys` stop times are `HHMMSS` local times (plus `utc_*`
  variants) without a date; the service date is in the vehicle journey id.

autocruise sends requests in UTC and returns RFC 3339 times with offset.

## Limits

- **No prices, no booking.** The API does not provide them (SNCF Connect has no public API).
- **Cancelled trains may disappear** from departure/arrival boards instead of
  being flagged. `sncf_disruptions` for the station lists them.
- Real-time data only exists for the near term; later trains show `realtime: false`.
- Requests are serialized with a 300 ms minimum interval; station lookups are
  cached in memory for one hour.
