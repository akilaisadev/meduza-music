import requests, json

url = "https://music.youtube.com/youtubei/v1/browse?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30"
data = {
    "browseId": "FEmusic_home",
    "context": {
        "client": {
            "clientName": "WEB_REMIX",
            "clientVersion": "1.20240501.01.00",
            "hl": "en",
            "gl": "US",
            "platform": "DESKTOP"
        }
    }
}
res = requests.post(url, json=data).json()
with open("home.json", "w") as f:
    json.dump(res, f, indent=2)
print("Done!")
