#!/usr/bin/env python3
"""Keycloak 26 setup: realm, client, user, protocol mappers, custom attributes."""

import json, urllib.request, urllib.parse, urllib.error, time, sys, os

KC = os.environ.get("KC_URL", "http://localhost:8080")
ADMIN = os.environ.get("KC_ADMIN", "admin:admin")

def req(method, path, data=None):
    hdr = {"Content-Type": "application/json"}
    url = f"{KC}{path}"
    if method == "GET" and data and not isinstance(data, (str, bytes)):
        url += "?" + urllib.parse.urlencode(data)
        data = None
    body = json.dumps(data).encode() if data else None
    r = urllib.request.Request(url, data=body, headers=hdr, method=method)
    if path.startswith("/admin"):
        r.add_header("Authorization", f"Bearer {token}")
    try:
        resp = urllib.request.urlopen(r)
        ct = resp.headers.get("Content-Type", "")
        return json.loads(resp.read()) if "json" in ct else resp.read()
    except urllib.error.HTTPError as e:
        if e.code == 204:
            return None
        if e.code == 409:
            return "CONFLICT"
        print(f"ERROR {method} {path}: {e.code} {e.reason}")
        print(e.read().decode())
        sys.exit(1)

print(f"Connecting to Keycloak at {KC} ...")

# Admin token
data = urllib.parse.urlencode({"client_id":"admin-cli","grant_type":"password","username":ADMIN.split(":")[0],"password":ADMIN.split(":")[1]}).encode()
token = json.loads(urllib.request.urlopen(urllib.request.Request(f"{KC}/realms/master/protocol/openid-connect/token", data=data, headers={"Content-Type":"application/x-www-form-urlencoded"})).read())["access_token"]
print("Got admin token")

# Create realm (ignore if exists)
if req("POST", "/admin/realms", {"realm":"morphis","enabled":True}) != "CONFLICT":
    print("Realm 'morphis' created")
else:
    print("Realm 'morphis' already exists")

# Create client (ignore if exists)
if req("POST", "/admin/realms/morphis/clients", {
    "clientId":"morphis-test","enabled":True,"publicClient":False,
    "secret":"morphis-test-secret","directAccessGrantsEnabled":True,
    "protocol":"openid-connect","standardFlowEnabled":False
}) != "CONFLICT":
    print("Client 'morphis-test' created")
else:
    print("Client 'morphis-test' already exists")

# Register custom attributes in User Profile (Keycloak 26 requirement)
time.sleep(2)
profile = req("GET", "/admin/realms/morphis/users/profile")
existing = {a["name"] for a in profile["attributes"]}
for attr in [
    {"name":"tenant_id","displayName":"Tenant ID",
     "permissions":{"view":["admin","user"],"edit":["admin","user"]},"multivalued":False},
    {"name":"role","displayName":"Role",
     "permissions":{"view":["admin","user"],"edit":["admin","user"]},"multivalued":False}
]:
    if attr["name"] not in existing:
        profile["attributes"].append(attr)
req("PUT", "/admin/realms/morphis/users/profile", profile)
print("User profile attributes registered")

# Create or update user (support both fresh and --import-realm scenarios)
users = req("GET", "/admin/realms/morphis/users", {"username":"testuser"})
user_data = {
    "username":"testuser","email":"testuser@morphis.test",
    "firstName":"Test","lastName":"User","emailVerified":True,"enabled":True,
    "attributes":{"tenant_id":["test-tenant"],"role":["admin"]}
}
if users:
    uid = users[0]["id"]
    req("PUT", f"/admin/realms/morphis/users/{uid}", user_data)
    print(f"User 'testuser' updated (id={uid})")
else:
    uid = None  # will be extracted from response
    resp = req("POST", "/admin/realms/morphis/users", user_data)
    # Get user ID by searching again
    users = req("GET", "/admin/realms/morphis/users", {"username":"testuser"})
    if users and "id" in users[0]:
        uid = users[0]["id"]
    print(f"User 'testuser' created (id={uid})")

# Set password (works whether user was created or updated)
if uid:
    req("PUT", f"/admin/realms/morphis/users/{uid}/reset-password",
        {"type":"password","value":"testpass","temporary":False})
    print("Password set for testuser")
else:
    print("ERROR: Could not determine user ID")
    sys.exit(1)

# Add protocol mappers to client for custom claims in JWT
clients = req("GET", "/admin/realms/morphis/clients", {"clientId":"morphis-test"})
cid = clients[0]["id"]
for mapper in [
    {"name":"tenant_id","protocol":"openid-connect",
     "protocolMapper":"oidc-usermodel-attribute-mapper",
     "config":{"user.attribute":"tenant_id","claim.name":"tenant_id","jsonType.label":"String",
               "id.token.claim":True,"access.token.claim":True,"userinfo.token.claim":True}},
    {"name":"role","protocol":"openid-connect",
     "protocolMapper":"oidc-usermodel-attribute-mapper",
     "config":{"user.attribute":"role","claim.name":"role","jsonType.label":"String",
               "id.token.claim":True,"access.token.claim":True,"userinfo.token.claim":True}}
]:
    if req("POST", f"/admin/realms/morphis/clients/{cid}/protocol-mappers/models", mapper) == "CONFLICT":
        print(f"  Protocol mapper '{mapper['name']}' already exists")
    else:
        print(f"  Protocol mapper '{mapper['name']}' added")
print("Protocol mappers added")

print("\nKeycloak setup complete!")
