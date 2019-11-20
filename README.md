<p align="center"><img src="/resources/Design%205.PNG" alt="MOZAICP"/></p>

# MOZAICP (silent P)

MOZAIC is the **M**assive **O**nline **Z**eus **A**rtificial **I**ntelligence **C**ompetition **P**latform.
It aims to provide a flexible platform to host your very own AI competition. Just plug your game, and off you go!

Eventually MOZAIC should be very modular, so that you can provide a custom-tailored experience for your competitors, without having to worry about the heavy lifting.

## Contact

Have any questions, comments, want to contribute or are even remotely interested in this project, please get in touch!
You can reach us by [e-mail](mailto:bottlebats@zeus.ugent.be), [Facebook](https://www.facebook.com/zeus.wpi), or any other way you prefer listed [here](https://zeus.ugent.be/about/).

## Current goals

- Remove capnproto this keeps us down, it's not logical that you would have to decompose and recompose capnproto messages per reactor stop. If not capnproto we need something else, but I think it is possible to just accept everything with 'an ID'. You just have to transport 2 things, a typeid, and a pointer. Yes it is ugly ish, yes it is cool and probably fast.

- Try to remove Broker from the equation, I think it would be way better to have a more 'genetic' structure, where a link wraps a mpsc::Sender<M> directly to the reactor.

- Cascade on delete. On of the many challenges is that it is very hard to clean everything up. It is possible for sure, but not expected of end users. It would be nice to have a cascade on delete function, so reactors can open links that when closed they are closed.

## Flow

MOZAICP consists of multiple big components.

Reactors are the nodes in your genetic network. They consist of a state `S` and handlers for internal messages, `HashMap<TypeId, ReactorHandler>`. A reactor also has links to other reactors. `HashMap<ReactorId, Link>`.

A Link is a one way link between Reactors, it should at all times be matched with another link in the opposite direction. A link consists of a state `S` and handlers for internal and external messages, both `HashMap<TypeId, LinkHandler>`.

Handler is most of the time a pair of a type T and a function that uses a context C and that T. It has a function handle that accepts a context C. a type M (Runtime specific) that gets transformed into a T and is handled by that function.

A ReactorHandler is a Handler with a ReactorContext. This context holds that reactor state S and a ReactorHandle that can open links and spawn new Reactors and send 'internal' messages.

A LinkHandler is a Handler with a LinkContext. This context holds that link state S and a LinkHandle that can close that link, send 'internal' messages to its owning reactor, and send 'external' messages, to the other side of the link (via that mpsc::Sender<M>).

Internal messages get handled as follows: first the reactor tries to find a corresponding Handler. Next he sends the message to all his links to see if any of them handle that message.

External messages get handled as follows: the reactor reads that messages from his channel and looks for the corresponding link (this might fail). That links tries to handle that message with the correct handler (this might fail).

Everything is dependant on what messages can be transported, being the M type parameter. This is made clear with the Broker I guess.
A Broker only distributes the mpsc::Sender<M> for all reactors in that Broker.
