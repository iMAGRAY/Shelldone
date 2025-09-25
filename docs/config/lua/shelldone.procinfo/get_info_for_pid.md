# `shelldone.procinfo.get_info_for_pid(pid)`

{{since('20220807-113146-c2fee766')}}

Returns a [LocalProcessInfo](../LocalProcessInfo.md) object for the specified
process id.

This function may return `nil` if it was unable to return the info.

```
> shelldone.procinfo.get_info_for_pid(shelldone.procinfo.pid())
{
    "argv": [
        "/home/shelldone/shelldone-labs/shelldone/target/debug/shelldone-gui",
    ],
    "children": {
        540513: {
            "argv": [
                "-zsh",
            ],
            "children": {},
            "cwd": "/home/shelldone",
            "executable": "/usr/bin/zsh",
            "name": "zsh",
            "pid": 540513,
            "ppid": 540450,
            "start_time": 232656896,
            "status": "Sleep",
        },
    },
    "cwd": "/home/shelldone/shelldone-labs/shelldone",
    "executable": "/home/shelldone/shelldone-labs/shelldone/target/debug/shelldone-gui",
    "name": "shelldone-gui",
    "pid": 540450,
    "ppid": 425276,
    "start_time": 8671498240,
    "status": "Run",
}
```
