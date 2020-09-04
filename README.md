# What is this

A scraper for mangadex.

# Usage

```
Usage:
  mdscrape [OPTIONS] RESOURCE ID

Scraper for mangadex.org

Positional arguments:
  resource id           The resource id (the number in the URL)

Optional arguments:
  -h,--help             Show this help message and exit
  -v,--verbose          Be verbose
  --no-progress         Don't report progress
  -c,--chapter          Download a single manga chapter
  -t,--title            Download an entire manga title
  -l,--lang-code LANG_CODE
                        The language code, defaults to gb (Great
                        Britain/English)
  -s,--start-chapter START_CHAPTER
                        First chapter to download for a title
  -e,--end-chapter END_CHAPTER
                        Last chapter to download for a title
  -i,--info             Only print info about the chapter
  -g,--global-threshold GLOBAL_THRESHOLD
                        Max number of simultaneous connections
  -p,--per-origin-threshold PER_ORIGIN_THRESHOLD
                        Max number of simultaneous connections per origin
  --ignored-groups IGNORED_GROUPS
                        Groups not to download chapters from, separated by
                        commas
```
