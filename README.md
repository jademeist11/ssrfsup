# SSRFs Up!
**ssrfsup** is a Server-Side Request Forgery (SSRF) scanner for Yahoo! External Cache (EC) proxies. It works by checking visibility of each provided URL through three different lenses: direct external request, request proxied through EC proxy at https://ec.yimg.com/ec, and request proxied through the Mail EC proxy at https://ecp.yusercontent.com/mail. You can learn more about the EC proxies at https://ouryahoo.atlassian.net/wiki/spaces/EDGEOPS/pages/112984576/YCS-EC+Overview. 

## Synopsis
```
Usage: ssrfsup [OPTIONS] --input <INPUT> --output <OUTPUT>

Options:
  -i, --input <INPUT>
  -o, --output <OUTPUT>
      --threads <THREADS>  [default: 10]
      --timeout <TIMEOUT>  [default: 30]
  -h, --help               Print help
  -V, --version            Print version
```

## Prerequisites
If you do not already have Rust installed, install Rust with https://rustup.rs/. If you already have Rust installed, ensure it is current by running `rustup update`.

You will need access to the EC signing keys in `CKMS`. You will need access to both shared keys: [ycs.ec.urlsig](https://ui.ckms.ouroath.com/aws/view-keygroup/ycs.ec/view-key/ycs.ec.urlsig/versions) and [ycs.ec.mail.urlsig](https://ui.ckms.ouroath.com/aws/view-keygroup/ycs.ec/view-key/ycs.ec.mail.urlsig/versions). To request access to these keys, go to  [https://ui.ckms.ouroath.com/](https://ui.ckms.ouroath.com/)

-   For AWS client access, Click ["Add New Member"](https://ui.ckms.ouroath.com/aws/view-keygroup/traffic.ycs.ec/members)  add athenz service (az-domain.az-service)
-   For On-Prem client access, click  ["Add New Member"](https://ui.ckms.ouroath.com/prod/view-keygroup/traffic.ycs.ec/members)  add athenz service (az-domain.az-service)
-   Use one of the client access method in  [ckms guide](https://git.vzbuilders.com/pages/ykeykey/ckms-guide/key_access_clients/ykeykeyd/)
-   References:
    -   [https://thestreet.vzbuilders.com/thestreet/ls/community/tech-central/post/5407454474797056](https://thestreet.vzbuilders.com/thestreet/ls/community/tech-central/post/5407454474797056)
    -   [https://git.ouroath.com/pages/ykeykey/ckms-guide/go_secret_cli](https://git.ouroath.com/pages/ykeykey/ckms-guide/go_secret_cli/)

Once access has been granted, these keys can be manually retrieved using [`ckms-remotecli`](https://git.ouryahoo.com/ykeykey/ckms-remotecli):
```
ckms-remotecli -tlscert ~/.athenz/cert -tlskey ~/.athenz/key -group traffic.ycs.ec -key traffic.ycs.ec.urlsig -env aws

ckms-remotecli -tlscert ~/.athenz/cert -tlskey ~/.athenz/key -group traffic.ycs.ec -key traffic.ycs.ec.mail.urlsig -env aws
```

## Building & Installing
You can build and install **ssrfsup** with Cargo:
```
cargo check && cargo install --path .
```

## Output Schema
**ssrfsup** outputs results in JavaScript Object Notation (JSON) format. The schema of this JSON document can be described using the following OpenAPI schema component:
```
components:
  schemas:
    Result:
      type: object
      properties:
        external_status:
          type: integer
          format: int32
          description: The HTTP status code from an external request.
        ec_proxy_status:
          type: integer
          format: int32
          description: The HTTP status code from the EC proxy server.
        mail_proxy_status:
          type: integer
          format: int32
          description: The HTTP status code from the mail proxy server.
      required:
        - external_status
        - ec_proxy_status
        - mail_proxy_status
```

## Operation
First, you will need to export the appropriate environment variables for the EC proxy signing keys:
```
export EC_URLSIG_KEY='value of ycs.ec.urlsig key from ckms'
export EC_URLSIG_KEY_VER='5'
export EC_MAIL_URLSIG_KEY='value of ycs.ec.mail.urlsig key from ckms'
export EC_MAIL_URLSIG_KEY_VER='4'
```

Next, create a plain ASCII text file containing the URLs you wish to scan, with one URL per line. Note that the line must contain a valid URL, not just a bare domain name. For example, `https://mail.yahoo.com/` is valid, but `mail.yahoo.com` is invalid. An example input file can be found in `examples/example.txt`. 

Then, run **ssrfsup** specifying the text file you created as the input file, as well as whatever you wish to name the output file:
```
ssrfsup --input examples/example.txt --output ssrfsup_results.json
```
Upon completion, this will create an output file in JSON format. 

Ideally, every external request should return `0` (Host Unreachable), while every proxy request should return `403` (Forbidden) or `502` (Bad Gateway). Any status codes outside of those values should be investigated. Obviously a `200` response would be very bad, but some codes like `404` (Not Found) are deceptively misleading. A `404` could mean the proxy server successfully connected to the server but could not locate the requested document, which would indicate that particular URL is vulnerable to SSRF.

You can use this `jq` one-liner to help identify requests that are potentially vulnerable to SSRF:
```
jq 'to_entries | map(select(.value | any(.[]; . != 403 and . != 502))) | from_entries' ssrfsup_results.json
```

## Performance
**ssrfsup** is both threaded and asynchronous, which enables it to be highly performant. By default, **ssrfsup** uses a conservative 10 threads with a 30 second request timeout. You can decrease **ssrfsup**'s execution times by increasing the number of threads and/or decreasing the request timeout value. Depending upon available resources, you may be able to use 150 threads with a 10 second request timeout, for example. 

To tune these parameters, specify the appropriate values for the `--threads` and `--timeout` command line options:
```
ssrfsup --threads 150 --timeout 10 --input examples/example.txt --output ssrfsup_results.json
```
Keep in mind that speed can sacrifice reliability, as too many threads or too short of a timeout value can generate false negatives. These parameters should be carefully selected, if not modified at all. 

