import requests, json

url = "https://music.youtube.com/youtubei/v1/next?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30"
data = {
    "videoId": "9cS2wv6AfHk",
    "isAudioOnly": True,
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
try:
    tabs = res["contents"]["singleColumnMusicWatchNextResultsRenderer"]["tabbedRenderer"]["watchNextTabbedResultsRenderer"]["tabs"]
    content = tabs[0]["tabRenderer"]["content"]
    queue = content["musicQueueRenderer"]["content"]["playlistPanelRenderer"]["contents"]
    print("Found", len(queue), "tracks in radio queue!")
except Exception as e:
    print("Error parsing radio:", e)
