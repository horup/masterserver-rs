Implements two servers.

one server, exposed on port ws://0.0.0.0:/8080, provides ability for connected clients to share info with eachother.
the concept is that a client might wish to tell about a game room that other clients can connect to.
concept is similar to the masterserver concept in Q3 and other old school FPS games where servers could publish their
'whereabouts'.

other server, exposed on port ws://0.0.0.0:/8081, provides a matchbox compatible signaling server using server/client architecture.

naming of these two servers are pending (one of the hard part of computerscience).