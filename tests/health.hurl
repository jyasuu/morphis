GET http://app:4000/health
HTTP 200
[Asserts]
body == "ok"
