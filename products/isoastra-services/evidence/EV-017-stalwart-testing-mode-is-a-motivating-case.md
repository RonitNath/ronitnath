---
id: EV-017
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice)
---
"the big importance of Stalwart is just as a testing domain. So the thing I imagine is there's an application and there's an email flow, like sign up or register or change password or email support, or I have a question or I want to open a ticket or whatever. They send the email or they receive notifications. […] Regularly, we do test this by using AgentMail, which was essentially a third party service because of inbox, and then we would use our API to the mailer. The mailer would send a request to Amazon SES, and SES would send the mail to a third party service, then we'd use our API credentials with the third party service to view that mail. Instead, Stalwart is an area where the agent can just look at the inbox and use that inbox to send emails to the application, receive emails from the application, and use it as a testing interface. That is the main interface that is actually necessary, and this was actually one of the motivating cases for why I wanted to do this rebuild. Because I realized that mailer was not built for this kind of workflow and being able to have downstream applications easily express whether they're in testing mode or their production and who they want their mails to be sent to in the first place."

Extends EV-010 with the *reason this bet exists*. The requirement is not "support a test transport"
— it is that a downstream application can **express its mode** (testing vs production) and its
delivery destination as configuration the services app understands, so an agent driving that app
gets a real inbox it can both read and send from. The old chain (mailer → SES → AgentMail →
third-party API to read) is replaced by Stalwart as the loop's endpoint.

This is a first-class T1 mailing requirement, not a dev-convenience nice-to-have: the owner names it
as one of the motivating cases for the rebuild, and names the current mailer as not built for it.
