use super::*;

#[cfg(feature = "socket-dns")]
use crate::socket::dns::Socket as DnsSocket;

#[cfg(feature = "socket-udp")]
use crate::socket::udp::Socket as UdpSocket;

impl InterfaceInner {
    pub(super) fn process_udp<'frame>(
        &mut self,
        sockets: &mut SocketSet,
        meta: PacketMeta,
        handled_by_raw_socket: bool,
        ip_repr: IpRepr,
        ip_payload: &'frame [u8],
    ) -> Option<Packet<'frame>> {
        let (src_addr, dst_addr) = (ip_repr.src_addr(), ip_repr.dst_addr());
        let transport_protocol = ip_repr.next_header();
        let udp_packet = UdpPacket::new_unchecked(ip_payload);
        check!(udp_packet.check_len_for_protocol(transport_protocol));

        #[cfg(feature = "socket-udp")]
        {
            let mut is_received = false;
            for udp_socket in sockets
                .items_mut()
                .filter_map(|i| UdpSocket::downcast_mut(&mut i.socket))
            {
                if udp_socket.transport_protocol() != transport_protocol {
                    continue;
                }
                let Ok(udp_repr) = UdpRepr::parse_with_protocol(
                    &udp_packet,
                    &src_addr,
                    &dst_addr,
                    &self.caps.checksum,
                    transport_protocol,
                    udp_socket.udplite_recv_checksum_coverage(),
                ) else {
                    continue;
                };
                if udp_socket.accepts(self, &ip_repr, &udp_repr) {
                    udp_socket.process(
                        self,
                        meta,
                        &ip_repr,
                        &udp_repr,
                        udp_packet.payload_for_protocol(transport_protocol),
                    );
                    is_received = true;
                }
            }
            if is_received {
                return None;
            }
        }

        #[cfg(feature = "socket-dns")]
        for dns_socket in sockets
            .items_mut()
            .filter_map(|i| DnsSocket::downcast_mut(&mut i.socket))
        {
            if transport_protocol != IpProtocol::Udp {
                continue;
            }
            let udp_repr = check!(UdpRepr::parse(
                &udp_packet,
                &src_addr,
                &dst_addr,
                &self.caps.checksum
            ));
            if dns_socket.accepts(&ip_repr, &udp_repr) {
                dns_socket.process(self, &ip_repr, &udp_repr, udp_packet.payload());
                return None;
            }
        }

        // The packet wasn't handled by a socket, send an ICMP port unreachable packet.
        match ip_repr {
            #[cfg(feature = "proto-ipv4")]
            IpRepr::Ipv4(_) if handled_by_raw_socket => None,
            #[cfg(feature = "proto-ipv6")]
            IpRepr::Ipv6(_) if handled_by_raw_socket => None,
            #[cfg(feature = "proto-ipv4")]
            IpRepr::Ipv4(ipv4_repr) => {
                let payload_len =
                    icmp_reply_payload_len(ip_payload.len(), IPV4_MIN_MTU, ipv4_repr.buffer_len());
                let icmpv4_reply_repr = Icmpv4Repr::DstUnreachable {
                    reason: Icmpv4DstUnreachable::PortUnreachable,
                    header: ipv4_repr,
                    data: &ip_payload[0..payload_len],
                };
                self.icmpv4_reply(ipv4_repr, icmpv4_reply_repr)
            }
            #[cfg(feature = "proto-ipv6")]
            IpRepr::Ipv6(ipv6_repr) => {
                let payload_len =
                    icmp_reply_payload_len(ip_payload.len(), IPV6_MIN_MTU, ipv6_repr.buffer_len());
                let icmpv6_reply_repr = Icmpv6Repr::DstUnreachable {
                    reason: Icmpv6DstUnreachable::PortUnreachable,
                    header: ipv6_repr,
                    data: &ip_payload[0..payload_len],
                };
                self.icmpv6_reply(ipv6_repr, icmpv6_reply_repr)
            }
        }
    }
}
