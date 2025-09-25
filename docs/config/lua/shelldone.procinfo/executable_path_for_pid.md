# `shelldone.procinfo.executable_path_for_pid(pid)`

{{since('20220807-113146-c2fee766')}}

Returns the path to the executable image for the specified process id.

This function may return `nil` if it was unable to return the info.

```
> shelldone.procinfo.executable_path_for_pid(shelldone.procinfo.pid())
"/home/shelldone/shelldone-labs/shelldone/target/debug/shelldone-gui"
```

