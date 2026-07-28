import requests, json

url = "https://music.youtube.com/youtubei/v1/browse?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30"
data = {
    "browseId": "FEmusic_explore",
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
tabs = res.get("contents",{}).get("singleColumnBrowseResultsRenderer",{}).get("tabs",[])
if tabs:
    contents = tabs[0].get("tabRenderer",{}).get("content",{}).get("sectionListRenderer",{}).get("contents",[])
    print(len(contents), "sections")
    for sec in contents:
        print(sec.keys())
